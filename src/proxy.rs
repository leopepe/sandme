//! HTTP forward proxy mediating the sandboxed command's network egress (FR-005).
//!
//! The proxy listens on loopback only and forwards traffic to its original
//! destination without inspecting or filtering it — routing, not policy.

use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};

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
    }
}

/// Start the proxy on `port` of the loopback interface.
///
/// Binding happens synchronously so a startup failure reaches the caller
/// before the command is launched — a sandbox without its proxy is a broken
/// sandbox, so the whole invocation fails.
pub fn serve(port: u16) -> Result<Server, SandmeError> {
    let wanted = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let std_listener = std::net::TcpListener::bind(wanted)
        .map_err(|source| SandmeError::ProxyStartup { port, source })?;
    std_listener
        .set_nonblocking(true)
        .map_err(|source| SandmeError::ProxyStartup { port, source })?;
    let addr = std_listener
        .local_addr()
        .map_err(|source| SandmeError::ProxyStartup { port, source })?;

    let listener = TcpListener::from_std(std_listener)
        .map_err(|source| SandmeError::ProxyStartup { port, source })?;
    let accept_loop = tokio::spawn(accept_loop(listener));
    Ok(Server { addr, accept_loop })
}

/// Accept connections until the listener is aborted.
async fn accept_loop(listener: TcpListener) {
    while let Ok((stream, _)) = listener.accept().await {
        tokio::spawn(serve_connection(TokioIo::new(stream)));
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
        Ok(open_tunnel(req))
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

/// Answer a CONNECT request by copying bytes between client and target.
fn open_tunnel(req: Request<Incoming>) -> Response<Body> {
    let target = req.uri().authority().map(ToString::to_string);
    let upgraded = hyper::upgrade::on(req);

    tokio::spawn(async move {
        let Some(target) = target else { return };
        let (Ok(client), Ok(mut upstream)) = (upgraded.await, TcpStream::connect(&target).await)
        else {
            return;
        };
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
