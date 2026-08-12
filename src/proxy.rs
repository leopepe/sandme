//! HTTP forward proxy mediating the sandboxed command's network egress (FR-005).
//!
//! The proxy listens on loopback only and forwards traffic to its original
//! destination without inspecting or filtering it — routing, not policy.

use std::convert::Infallible;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};

use http_body_util::{BodyExt, Empty, combinators::BoxBody};
use hyper::body::{Bytes, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::client::legacy::Client;
use hyper_util::rt::{TokioExecutor, TokioIo};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::error::SandmeError;

/// The body type returned by the proxy.
type Body = BoxBody<Bytes, hyper::Error>;

/// A running proxy server; stopping is tied to this handle's lifetime.
///
/// When the handle is dropped the accept loop is aborted, which is how the
/// proxy's lifetime follows the sandboxed command's (T-007).
pub struct Server {
    addr: SocketAddr,
    accept_loop: JoinHandle<()>,
    accept_loop_v6: Option<JoinHandle<()>>,
}

impl Server {
    /// The socket the sandboxed command's egress is routed to.
    pub fn addr(&self) -> SocketAddr {
        self.addr
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

/// Start the proxy on `port` of the loopback interface.
///
pub fn serve(port: u16) -> Result<Server, SandmeError> {
    let fail = |source| SandmeError::ProxyStartup { port, source };

    // Bind to IPv4 loopback; some HTTP clients (e.g. Zed's reqwest) try
    // IPv6 ::1 first and don't fall back, so we also bind IPv6 loopback.
    let v4 = std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        .map_err(&fail)?;
    v4.set_nonblocking(true).map_err(&fail)?;
    let addr = v4.local_addr().map_err(&fail)?;

    let v4_listener = TcpListener::from_std(v4).map_err(&fail)?;
    let accept_loop = tokio::spawn(run_accept_loop(v4_listener));

    let accept_loop_v6 = std::net::TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, port)))
        .ok()
        .and_then(|s| {
            let _ = s.set_nonblocking(true);
            TcpListener::from_std(s).ok()
        })
        .map(|l| tokio::spawn(run_accept_loop(l)));

    Ok(Server {
        addr,
        accept_loop,
        accept_loop_v6,
    })
}

/// Accept connections until the listener is aborted.
async fn run_accept_loop(listener: TcpListener) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(serve_connection(TokioIo::new(stream)));
            }
            Err(error) => {
                eprintln!("sandme: proxy accept error: {error}");
                return;
            }
        }
    }
}

/// Serve one client connection, honouring CONNECT upgrades.
async fn serve_connection(io: TokioIo<TcpStream>) {
    let conn = http1::Builder::new()
        .serve_connection(io, service_fn(route))
        .with_upgrades();
    if let Err(error) = conn.await {
        eprintln!("sandme: proxy connection error: {error}");
    }
}

/// Route one request: CONNECT opens a tunnel, anything else is forwarded.
async fn route(req: Request<Incoming>) -> Result<Response<Body>, Infallible> {
    if req.method() == Method::CONNECT {
        Ok(open_tunnel(req).await)
    } else {
        Ok(forward(req).await)
    }
}

/// Forward an absolute-form request to its origin and return the answer.
///
/// Failure to reach the origin becomes `502 Bad Gateway` for the client —
/// the proxy reports it instead of sandme aborting the invocation.
async fn forward(req: Request<Incoming>) -> Response<Body> {
    let client = Client::builder(TokioExecutor::new()).build_http();
    match client.request(req).await {
        Ok(response) => response.map(BodyExt::boxed),
        Err(_) => response_with(StatusCode::BAD_GATEWAY),
    }
}

/// Answer a CONNECT request, committing only once the tunnel is live.
///
/// The upstream connection is established before the `200` goes out, so a
/// refused target answers `502` instead of a dead tunnel; a CONNECT without
/// an authority answers `400`. Once both ends are up, bytes are copied in
/// both directions until either side closes.
async fn open_tunnel(req: Request<Incoming>) -> Response<Body> {
    let Some(target) = req.uri().authority().map(ToString::to_string) else {
        return response_with(StatusCode::BAD_REQUEST);
    };

    let Ok(mut upstream) = TcpStream::connect(&target).await else {
        return response_with(StatusCode::BAD_GATEWAY);
    };

    let upgraded = hyper::upgrade::on(req);
    tokio::spawn(async move {
        let Ok(client) = upgraded.await else { return };
        let mut client = TokioIo::new(client);
        let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
    });

    response_with(StatusCode::OK)
}

/// Build an empty response with the given status.
fn response_with(status: StatusCode) -> Response<Body> {
    let body = Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed();
    let mut response = Response::new(body);
    *response.status_mut() = status;
    response
}
