//! Tunnels git's SSH transport through sandme's HTTP `CONNECT` proxy (issue #33).
//!
//! The sandbox profile permits one network destination, the proxy, so `ssh`
//! to port 22 is denied and `git@github.com:…` fails. `ssh` can instead reach
//! its server through an HTTP `CONNECT` tunnel, which the proxy already speaks.
//! This module has two halves of one contract: it builds the `GIT_SSH_COMMAND`
//! that points git's `ssh` at a `ProxyCommand`, and it *is* that
//! `ProxyCommand` — sandme re-executed in a mode that opens the tunnel. Keeping
//! both here keeps the sentinel, the argument order and the proxy protocol in
//! one place; splitting them would let the two ends drift apart.

use std::net::SocketAddr;
use std::path::Path;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use crate::credential;
use crate::error::SandmeError;

/// The first argument that puts sandme into `ProxyCommand` mode.
///
/// It is not a documented flag: sandme sets it on itself through
/// `GIT_SSH_COMMAND`, and `main` intercepts it before the CLI is parsed. The
/// two arguments after it are ssh's `%h` (host) and `%p` (port).
pub const PROXY_COMMAND_ARG: &str = "--sandme-ssh-connect";

/// The `GIT_SSH_COMMAND` that routes git's `ssh` through sandme's proxy.
///
/// git runs this string through a shell, which passes ssh a `ProxyCommand`
/// that re-executes sandme in tunnel mode. The `ProxyCommand` reads the
/// proxy's address and credential from `HTTP_PROXY` — the same variable sandme
/// already sets on the child (SPEC-0003 FR-203) — so no secret is written into
/// the command line, where another process could read it.
///
/// `%h` and `%p` are ssh's own placeholders for the destination host and port;
/// ssh substitutes them before running the `ProxyCommand`. The `ProxyCommand`
/// is single-quoted so git's shell passes it to ssh as one argument.
pub fn git_ssh_command(sandme_exe: &Path) -> String {
    // The executable path sits inside single quotes for git's shell layer;
    // ssh then re-splits the value on spaces, so a space in the path would
    // break the inner layer. sandme's own install path having no shell-special
    // character is the assumption recorded in SPEC-0012.
    format!(
        "ssh -o ProxyCommand='{} {PROXY_COMMAND_ARG} %h %p'",
        sandme_exe.display()
    )
}

/// Open the tunnel to `host`:`port` and splice it to this process's stdio.
///
/// This runs as ssh's `ProxyCommand`: ssh writes SSH-protocol bytes to our
/// stdin and reads the server's from our stdout, and this function relays each
/// to the other end of an HTTP `CONNECT` tunnel through sandme's proxy.
///
/// # Errors
///
/// Returns a `Tunnel*` [`SandmeError`] when the proxy configuration is
/// missing, the proxy cannot be reached, or the proxy declines the `CONNECT`.
pub async fn run(host: &str, port: &str) -> Result<(), SandmeError> {
    let proxy = ProxyEndpoint::from_environment().ok_or(SandmeError::TunnelProxyUnset)?;

    let mut stream = TcpStream::connect(proxy.address).await.map_err(|source| {
        SandmeError::TunnelUnreachable {
            proxy: proxy.address,
            source,
        }
    })?;

    open_connect(&mut stream, &proxy.authorization, host, port).await?;
    splice_stdio(stream).await;
    Ok(())
}

/// sandme's proxy as an SSH `ProxyCommand` reads it from the environment.
struct ProxyEndpoint {
    /// The loopback socket the proxy listens on.
    address: SocketAddr,
    /// The `Proxy-Authorization` header value this invocation's child presents.
    authorization: String,
}

impl ProxyEndpoint {
    /// Parse `HTTP_PROXY` into the proxy's address and credential.
    ///
    /// sandme writes the variable as `http://sandme:<secret>@<addr>`
    /// (SPEC-0003), so the userinfo is the `Basic` credential and the
    /// authority is a loopback socket address. Anything that does not parse
    /// yields `None`, which the caller turns into [`SandmeError::TunnelProxyUnset`].
    fn from_environment() -> Option<Self> {
        let url = std::env::var("HTTP_PROXY")
            .or_else(|_| std::env::var("http_proxy"))
            .ok()?;
        let rest = url.strip_prefix("http://")?;
        let (userinfo, authority) = rest.split_once('@')?;
        let address = authority.parse().ok()?;

        Some(Self {
            address,
            authorization: format!("Basic {}", credential::base64(userinfo)),
        })
    }
}

/// Send the `CONNECT` request and read the proxy's answer.
///
/// The tunnel is committed only on a `2xx`; any other status is the proxy
/// declining (SPEC-0003 FR-201 answers `403` for a restricted destination,
/// `407` for a missing credential), reported so the user can act on it.
async fn open_connect(
    stream: &mut TcpStream,
    authorization: &str,
    host: &str,
    port: &str,
) -> Result<(), SandmeError> {
    let request = format!(
        "CONNECT {host}:{port} HTTP/1.1\r\nHost: {host}:{port}\r\n\
         Proxy-Authorization: {authorization}\r\n\r\n"
    );
    let proxy = stream
        .peer_addr()
        .unwrap_or_else(|_| SocketAddr::from(([0, 0, 0, 0], 0)));
    let unreachable = |source| SandmeError::TunnelUnreachable { proxy, source };

    stream
        .write_all(request.as_bytes())
        .await
        .map_err(unreachable)?;

    let status = read_status_line(stream).await.map_err(unreachable)?;
    if status.starts_with('2') {
        Ok(())
    } else {
        Err(SandmeError::TunnelRefused {
            host: host.to_string(),
            port: port.to_string(),
            status,
        })
    }
}

/// Read the response up to the blank line ending its headers, returning the
/// status code.
///
/// Bytes are read one at a time so nothing past the header terminator is
/// consumed: whatever follows is the first of the tunnelled SSH bytes, which
/// [`splice_stdio`] must still see. A `CONNECT` response is a few dozen bytes,
/// so the per-byte read costs nothing measurable.
async fn read_status_line(stream: &mut TcpStream) -> Result<String, std::io::Error> {
    let mut head = Vec::new();
    let mut byte = [0_u8; 1];
    while !head.ends_with(b"\r\n\r\n") {
        stream.read_exact(&mut byte).await?;
        head.push(byte[0]);
    }

    // "HTTP/1.1 200 Connection established" -> "200"; a malformed line yields
    // an empty status, which the caller treats as a refusal.
    let first_line = head.split(|&b| b == b'\r').next().unwrap_or_default();
    let status = first_line.split(|&b| b == b' ').nth(1).unwrap_or_default();
    Ok(String::from_utf8_lossy(status).into_owned())
}

/// Copy bytes both ways between the tunnel and this process's stdio until the
/// server closes the connection.
///
/// The download half (proxy to stdout) is awaited in the foreground: when the
/// server closes, the `ProxyCommand` is done and returns. The upload half runs
/// as a background task so ssh closing our stdin does not cut off bytes still
/// arriving from the server.
async fn splice_stdio(stream: TcpStream) {
    let (mut from_proxy, mut to_proxy) = stream.into_split();

    tokio::spawn(async move {
        let mut stdin = tokio::io::stdin();
        let _ = tokio::io::copy(&mut stdin, &mut to_proxy).await;
        let _ = to_proxy.shutdown().await;
    });

    let mut stdout = tokio::io::stdout();
    let _ = tokio::io::copy(&mut from_proxy, &mut stdout).await;
    let _ = stdout.flush().await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn builds_a_git_ssh_command_that_names_the_proxy_command_sentinel() {
        // Given the path to the sandme executable
        let exe = PathBuf::from("/usr/local/bin/sandme");

        // Then the GIT_SSH_COMMAND runs ssh with a ProxyCommand that
        // re-executes sandme in tunnel mode, quoted as one argument for git's
        // shell, and carries ssh's own %h/%p placeholders
        assert_eq!(
            git_ssh_command(&exe),
            "ssh -o ProxyCommand='/usr/local/bin/sandme --sandme-ssh-connect %h %p'"
        );
    }
}
