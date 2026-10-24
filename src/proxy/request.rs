//! Authenticate and route proxy requests to vetted destinations.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;

use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::body::{Bytes, Incoming};
use hyper::client::conn::http1 as http1_client;
use hyper::header::{
    CONNECTION, HOST, HeaderMap, HeaderName, HeaderValue, PROXY_AUTHENTICATE, PROXY_AUTHORIZATION,
    TE, TRAILER, TRANSFER_ENCODING, UPGRADE,
};
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;

use super::egress::{self, Destination};

/// The body type returned by the proxy.
pub(super) type Body = BoxBody<Bytes, hyper::Error>;

/// What the proxy relays, and for whom (SPEC-0003).
#[derive(Clone)]
pub(super) struct Policy {
    /// The `Proxy-Authorization` header value this run's child will present.
    pub(super) expected_authorization: Arc<str>,
    /// Whether the host's own networks are relayed to at all (FR-205).
    pub(super) allow_private_egress: bool,
}

impl Policy {
    /// Whether the request carries this invocation's credential (FR-203).
    fn authorizes(&self, req: &Request<Incoming>) -> bool {
        req.headers()
            .get(PROXY_AUTHORIZATION)
            .is_some_and(|presented| presented.as_bytes() == self.expected_authorization.as_bytes())
    }
}

/// Route one request: refuse it, tunnel it, or forward it.
///
/// The credential is checked before the destination, so a request from
/// anything but this run's child learns nothing about what the proxy would
/// have done with it.
pub(super) async fn route(
    req: Request<Incoming>,
    policy: Policy,
) -> Result<Response<Body>, Infallible> {
    if !policy.authorizes(&req) {
        return Ok(unauthorized());
    }
    let Some((host, port)) = destination_of(&req) else {
        return Ok(response_with(
            StatusCode::BAD_REQUEST,
            "proxy received a request with no destination",
        ));
    };

    Ok(
        match egress::resolve(&host, port, policy.allow_private_egress).await {
            Destination::Permitted(addresses) => {
                if req.method() == Method::CONNECT {
                    open_tunnel(req, &addresses).await
                } else {
                    forward(req, &addresses).await
                }
            }
            Destination::Restricted => refuse(&host, port),
            Destination::Unresolvable => response_with(
                StatusCode::BAD_GATEWAY,
                &format!("proxy could not resolve {host}"),
            ),
        },
    )
}

/// The host and port a request names, with the scheme's default port.
///
/// An IPv6 literal reaches us bracketed (`[::1]`); the brackets are part of
/// the URI syntax, not of the address, and a resolver rejects them.
fn destination_of<B>(req: &Request<B>) -> Option<(String, u16)> {
    let authority = req.uri().authority()?;
    let default_port = if req.method() == Method::CONNECT {
        443
    } else {
        80
    };
    let host = authority.host().trim_matches(['[', ']']).to_string();
    Some((host, authority.port_u16().unwrap_or(default_port)))
}

/// Answer a refused destination, and say so on stderr (FR-201, FR-202).
///
/// The stderr line is there because the body is not always shown: a client
/// that sent `CONNECT` reports the status and drops the body, and a user
/// staring at a failing agent needs to be told which setting governs it.
fn refuse(host: &str, port: u16) -> Response<Body> {
    let reason = egress::RESTRICTED_DESTINATION;
    eprintln!("sandme: proxy refused {host}:{port}: {reason}");
    response_with(
        StatusCode::FORBIDDEN,
        &format!("proxy refused {host}:{port}: {reason}"),
    )
}

/// Answer a request that did not come from this invocation's child (FR-203).
///
/// The challenge is included so a client configured through `HTTP_PROXY`
/// retries with its credential instead of failing opaquely.
fn unauthorized() -> Response<Body> {
    let mut response = response_with(
        StatusCode::PROXY_AUTHENTICATION_REQUIRED,
        "proxy refused a request that did not come from this invocation's sandboxed command",
    );
    response.headers_mut().insert(
        PROXY_AUTHENTICATE,
        HeaderValue::from_static("Basic realm=\"sandme\""),
    );
    response
}

/// Forward an absolute-form request to its origin and return the answer.
///
/// The connection is opened to the addresses [`egress::resolve`] already
/// vetted, not to the name again: re-resolving on the forward path is the
/// DNS-rebind window `open_tunnel` never had, and closing it holds the whole
/// proxy to one lookup per request (FR-1101, NFR-1101).
///
/// Hop-by-hop headers — the credential among them — are stripped and `Host` is
/// pinned to the vetted authority first, so the origin receives only what the
/// proxy checked and never the credential (FR-1102, FR-1103, FR-204).
///
/// Failure to reach the origin becomes `502 Bad Gateway` for the client —
/// the proxy reports it instead of sandme aborting the invocation.
async fn forward(mut req: Request<Incoming>, addresses: &[SocketAddr]) -> Response<Body> {
    let unreachable = || response_with(StatusCode::BAD_GATEWAY, "proxy could not reach the origin");

    strip_hop_by_hop(req.headers_mut());
    to_origin_form(&mut req);

    let Ok(stream) = TcpStream::connect(addresses).await else {
        return unreachable();
    };
    let Ok((mut sender, conn)) = http1_client::handshake(TokioIo::new(stream)).await else {
        return unreachable();
    };
    tokio::spawn(conn);

    match sender.send_request(req).await {
        Ok(response) => response.map(BodyExt::boxed),
        Err(_) => unreachable(),
    }
}

/// Rewrite an absolute-form proxied request to the origin form an origin
/// server expects, pinning `Host` to the authority the proxy vetted.
///
/// A proxied request names its target in absolute form (`GET http://host/…`);
/// an origin server is sent origin form (`GET /…`) with the host in a `Host`
/// header. Setting that header from the vetted authority — rather than trusting
/// the one the client sent — stops the client selecting a vhost the proxy did
/// not check (FR-1103).
fn to_origin_form(req: &mut Request<Incoming>) {
    if let Some(authority) = req.uri().authority().cloned()
        && let Ok(host) = HeaderValue::from_str(authority.as_str())
    {
        req.headers_mut().insert(HOST, host);
    }
    let path = req
        .uri()
        .path_and_query()
        .map_or_else(|| "/".to_string(), |path| path.as_str().to_string());
    if let Ok(uri) = path.parse::<hyper::Uri>() {
        *req.uri_mut() = uri;
    }
}

/// Remove the hop-by-hop header fields a proxy must not forward (RFC 9110
/// §7.6.1): the fixed set below, and any field a `Connection` header names.
fn strip_hop_by_hop(headers: &mut HeaderMap) {
    let connection_named: Vec<HeaderName> = headers
        .get_all(CONNECTION)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .filter_map(|name| name.trim().parse().ok())
        .collect();

    for name in connection_named {
        headers.remove(name);
    }
    for name in hop_by_hop_headers() {
        headers.remove(name);
    }
}

/// The hop-by-hop header fields named by RFC 9110 §7.6.1, plus the widely sent
/// non-standard `Proxy-Connection`. Meaningful only on the hop they arrived on,
/// so none is relayed (FR-1102).
fn hop_by_hop_headers() -> [HeaderName; 9] {
    [
        CONNECTION,
        PROXY_AUTHENTICATE,
        PROXY_AUTHORIZATION,
        TE,
        TRAILER,
        TRANSFER_ENCODING,
        UPGRADE,
        HeaderName::from_static("keep-alive"),
        HeaderName::from_static("proxy-connection"),
    ]
}

/// Answer a CONNECT request, committing only once the tunnel is live.
///
/// The upstream connection is established before the `200` goes out, so a
/// refused target answers `502` instead of a dead tunnel. It is made to the
/// addresses [`egress::resolve`] vetted, not to the name again: re-resolving would
/// let a second answer replace the one that was checked. Once both ends are
/// up, bytes are copied in both directions until either side closes.
async fn open_tunnel(req: Request<Incoming>, addresses: &[SocketAddr]) -> Response<Body> {
    let Ok(mut upstream) = TcpStream::connect(addresses).await else {
        return response_with(
            StatusCode::BAD_GATEWAY,
            "proxy could not reach the destination",
        );
    };

    let upgraded = hyper::upgrade::on(req);
    tokio::spawn(async move {
        let Ok(client) = upgraded.await else { return };
        let mut client = TokioIo::new(client);
        let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    });

    response_with(StatusCode::OK, "")
}

/// Build a response with the given status and a message the user can act on.
fn response_with(status: StatusCode, message: &str) -> Response<Body> {
    let text = if message.is_empty() {
        String::new()
    } else {
        format!("sandme: {message}\n")
    };
    let body = Full::new(Bytes::from(text))
        .map_err(|never| match never {})
        .boxed();
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_destination_a_request_names() {
        // Given the three request forms a proxy sees
        let connect = Request::builder()
            .method(Method::CONNECT)
            .uri("example.com:443")
            .body(())
            .unwrap();
        let absolute = Request::builder()
            .uri("http://example.com/index.html")
            .body(())
            .unwrap();
        let bracketed = Request::builder()
            .uri("http://[::1]:8080/")
            .body(())
            .unwrap();

        // Then each yields the host and port the destination check needs,
        // with the scheme's default port and without the brackets an IPv6
        // literal wears in a URI — a resolver rejects those
        assert_eq!(
            destination_of(&connect),
            Some(("example.com".to_string(), 443))
        );
        assert_eq!(
            destination_of(&absolute),
            Some(("example.com".to_string(), 80))
        );
        assert_eq!(destination_of(&bracketed), Some(("::1".to_string(), 8080)));
    }
}
