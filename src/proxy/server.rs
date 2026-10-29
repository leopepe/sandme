//! The proxy's loopback listeners and running-server lifetime.

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::config::{Config, ProxyIpVersion};
use crate::credential::Credential;
use crate::error::SandmeError;

use super::request::{Policy, route};

/// A running proxy server; stopping is tied to this handle's lifetime.
///
/// When the handle is dropped, the accept loop is aborted, which is how the
/// proxy's lifetime follows the sandboxed command's (T-007).
pub struct Server {
    addr: SocketAddr,
    credential: Credential,
    accept_loop: JoinHandle<()>,
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
    }
}

/// Start the proxy on the configured port of the loopback interface.
pub fn serve(config: &Config) -> Result<Server, SandmeError> {
    let port = config.proxy_port;

    let credential = Credential::build();
    let policy = Policy {
        expected_authorization: Arc::from(credential.expected_authorization().as_str()),
        allow_private_egress: config.allow_private_egress,
    };

    let v4 = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let v6 = SocketAddr::from((Ipv6Addr::LOCALHOST, port));
    let (preferred, fallback) = match config.proxy_ip_version {
        ProxyIpVersion::Ipv4 => (v4, v6),
        ProxyIpVersion::Ipv6 => (v6, v4),
    };
    // Actual setup determines availability; spawning waits until one listener is ready
    // so a failed attempt leaves no live task or socket behind (FR-1902).
    let (listener, addr) = match bind_loopback(preferred) {
        Ok(bound) => bound,
        Err(preferred_error) => {
            let bound = match bind_loopback(fallback) {
                Ok(bound) => bound,
                Err(source) => {
                    return Err(SandmeError::ProxyStartup {
                        preferred,
                        preferred_error,
                        fallback,
                        source,
                    });
                }
            };
            eprintln!(
                "sandme: proxy could not listen on {preferred}: {preferred_error}; continuing on {}",
                bound.1
            );
            bound
        }
    };
    let accept_loop = tokio::spawn(run_accept_loop(listener, policy));

    Ok(Server {
        addr,
        credential,
        accept_loop,
    })
}

/// Bind a loopback listener, ready for the accept loop.
fn bind_loopback(addr: SocketAddr) -> std::io::Result<(TcpListener, SocketAddr)> {
    let listener = std::net::TcpListener::bind(addr)?;
    listener.set_nonblocking(true)?;
    let addr = listener.local_addr()?;
    Ok((TcpListener::from_std(listener)?, addr))
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
