//! The proxy's loopback listeners and running-server lifetime.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::config::Config;
use crate::credential::Credential;
use crate::error::SandmeError;

use super::request::{Policy, route};

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
    let v4 =
        std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).map_err(fail)?;
    v4.set_nonblocking(true).map_err(fail)?;
    let addr = v4.local_addr().map_err(fail)?;

    let v4_listener = TcpListener::from_std(v4).map_err(fail)?;
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
