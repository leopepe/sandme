//! HTTP forward proxy mediating the sandboxed command's network egress
//! (FR-005, SPEC-0003).
//!
//! The proxy listens on loopback, relays for the invocation that started it,
//! and refuses the destinations the sandbox profile denies the command
//! directly — without it, egress would be routed rather than restricted.

use std::convert::Infallible;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use http_body_util::{BodyExt, Full, combinators::BoxBody};
use hyper::body::{Bytes, Incoming};
use hyper::client::conn::http1 as http1_client;
use hyper::header::{
    CONNECTION, HOST, HeaderMap, HeaderName, HeaderValue, PROXY_AUTHENTICATE, PROXY_AUTHORIZATION,
    TE, TRAILER, TRANSFER_ENCODING, UPGRADE,
};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::config::Config;
use crate::credential::Credential;
use crate::egress::{self, Destination};
use crate::error::SandmeError;

/// The body type returned by the proxy.
type Body = BoxBody<Bytes, hyper::Error>;

/// A running proxy server; stopping is tied to this handle's lifetime.
///
/// When the handle is dropped the accept loop is aborted, which is how the
/// proxy's lifetime follows the sandboxed command's (T-007).
pub struct Server {
    addr: SocketAddr,
    credential: Credential,
    accept_loop: JoinHandle<()>,
    accept_loop_v6: Option<JoinHandle<()>>,
}

impl Server {
    /// The socket the sandboxed command's egress is routed to.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The proxy URL for the sandboxed command, carrying this run's
    /// credential — the child needs no configuration of its own (FR-203).
    pub fn url(&self) -> String {
        self.credential.proxy_url(self.addr)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.accept_loop.abort();
        if let Some(h) = self.accept_loop_v6.take() {
            h.abort();
        }
    }
}

/// What the proxy relays, and for whom (SPEC-0003).
#[derive(Clone)]
struct Policy {
    /// The `Proxy-Authorization` header value this run's child will present.
    expected_authorization: Arc<str>,
    /// Whether the host's own networks are relayed to at all (FR-205).
    allow_private_egress: bool,
}

impl Policy {
    /// Whether the request carries this invocation's credential (FR-203).
    fn authorizes(&self, req: &Request<Incoming>) -> bool {
        req.headers()
            .get(PROXY_AUTHORIZATION)
            .is_some_and(|presented| presented.as_bytes() == self.expected_authorization.as_bytes())
    }
}

/// Start the proxy on the configured port of the loopback interface.
pub fn serve(config: &Config) -> Result<Server, SandmeError> {
    let port = config.proxy_port;
    let fail = |source| SandmeError::ProxyStartup { port, source };

    let credential = Credential::build();
    let policy = Policy {
        expected_authorization: Arc::from(credential.expected_authorization().as_str()),
        allow_private_egress: config.allow_private_egress,
    };

    // Bind to IPv4 loopback; some HTTP clients (e.g. Zed's reqwest) try
    // IPv6 ::1 first and don't fall back, so we also bind IPv6 loopback.
    let v4 = std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        .map_err(&fail)?;
    v4.set_nonblocking(true).map_err(&fail)?;
    let addr = v4.local_addr().map_err(&fail)?;

    let v4_listener = TcpListener::from_std(v4).map_err(&fail)?;
    let accept_loop = tokio::spawn(run_accept_loop(v4_listener, policy.clone()));

    // Bind IPv6 to the port IPv4 actually got, not to `port`: with `port` 0 the
    // OS hands the second bind a different ephemeral port, and only `addr` is
    // published to the child and allowed by the sandbox profile.
    //
    // A failed IPv6 bind is warned about, not swallowed: a process squatting
    // `[::1]:port` would otherwise answer a client that reaches for IPv6 with
    // no sign to the user. The invocation still runs — the published URL is the
    // literal IPv4 loopback — so this warns and continues rather than failing
    // (FR-1105).
    let accept_loop_v6 = match bind_v6_loopback(addr.port()) {
        Ok(listener) => Some(tokio::spawn(run_accept_loop(listener, policy))),
        Err(error) => {
            eprintln!(
                "sandme: proxy could not also listen on IPv6 loopback [::1]:{}: {error}; \
                 continuing on IPv4 loopback only",
                addr.port()
            );
            None
        }
    };

    Ok(Server {
        addr,
        credential,
        accept_loop,
        accept_loop_v6,
    })
}

/// Bind the IPv6 loopback listener on `port`, ready for the accept loop.
///
/// Separate from the IPv4 bind because its failure is handled differently: the
/// IPv4 bind fails the invocation, this one only warns (FR-1105).
fn bind_v6_loopback(port: u16) -> std::io::Result<TcpListener> {
    let listener = std::net::TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, port)))?;
    listener.set_nonblocking(true)?;
    TcpListener::from_std(listener)
}

/// Accept connections until the listener is aborted.
async fn run_accept_loop(listener: TcpListener, policy: Policy) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(serve_connection(TokioIo::new(stream), policy.clone()));
            }
            Err(error) => {
                eprintln!("sandme: proxy accept error: {error}");
                return;
            }
        }
    }
}

/// Serve one client connection, honouring CONNECT upgrades.
async fn serve_connection(io: TokioIo<TcpStream>, policy: Policy) {
    let service = service_fn(move |req| route(req, policy.clone()));
    let conn = http1::Builder::new()
        .serve_connection(io, service)
        .with_upgrades();
    if let Err(error) = conn.await {
        eprintln!("sandme: proxy connection error: {error}");
    }
}

/// Route one request: refuse it, tunnel it, or forward it.
///
/// The credential is checked before the destination, so a request from
/// anything but this run's child learns nothing about what the proxy would
/// have done with it.
async fn route(req: Request<Incoming>, policy: Policy) -> Result<Response<Body>, Infallible> {
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

    fn config_on_an_ephemeral_port() -> Config {
        Config {
            proxy_port: 0,
            ..Config::default()
        }
    }

    #[tokio::test]
    async fn serves_both_loopback_families_on_the_published_port() {
        // Given the proxy started on an ephemeral port
        let server =
            serve(&config_on_an_ephemeral_port()).expect("loopback bind on port 0 cannot fail");
        let port = server.addr().port();

        // Then both loopback families answer on the port the child is told
        // about — the IPv6 listener is not off on a port of its own
        assert_ne!(port, 0, "an ephemeral bind resolves to a real port");
        assert!(
            std::net::TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).is_ok(),
            "IPv4 loopback should accept on {port}"
        );
        assert!(
            std::net::TcpStream::connect(SocketAddr::from((Ipv6Addr::LOCALHOST, port))).is_ok(),
            "IPv6 loopback should accept on {port}"
        );
    }

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
