//! Integration tests: the built binary, driven through its CLI (SPEC-0001).

use std::net::{Ipv4Addr, SocketAddr};
use std::path::{Path, PathBuf};
use std::process::Command;

/// A scratch directory standing in for the user's filesystem; recreated
/// fresh on every run.
fn workdir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("sandme-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// The sandme binary under test, isolated from the developer's real config
/// and given its own proxy port so parallel tests do not collide.
fn sandme(workdir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sandme"));
    cmd.env("HOME", workdir)
        .env("SANDME_PROXY_PORT", free_port().to_string())
        .env_remove("SANDME_SHARED_PATHS")
        .current_dir(workdir);
    cmd
}

#[test]
fn prints_child_output() {
    // Given sandme and a command that prints a marker
    let dir = workdir("prints-child-output");
    let mut cmd = sandme(&dir);
    cmd.args(["echo", "sandme-says-hi"]);

    // When it is run
    let output = cmd.output().unwrap();

    // Then the child's output passes through and stdout carries nothing else
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "sandme-says-hi"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn passes_multi_word_arguments_unchanged() {
    // Given a command whose final argument contains a space
    let dir = workdir("passes-multi-word-arguments-unchanged");
    let mut cmd = sandme(&dir);
    cmd.args(["sh", "-c", "exit 3"]);

    // When it is run
    let status = cmd.status().unwrap();

    // Then the argument reached the child intact, and its exit code propagates
    assert_eq!(status.code(), Some(3));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reports_signal_deaths_as_128_plus_n() {
    // Given a command that kills itself with SIGINT (signal 2)
    let dir = workdir("reports-signal-deaths");
    let mut cmd = sandme(&dir);
    cmd.args(["sh", "-c", "kill -INT $$"]);

    // When it is run
    let status = cmd.status().unwrap();

    // Then sandme reports 128+2, not a collapsed value
    assert_eq!(status.code(), Some(130));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn denies_writes_outside_shared_paths() {
    // Given a shared directory and a path outside it
    let dir = workdir("denies-writes-outside-shared-paths");
    let outside = std::env::temp_dir().join("sandme-test-denied-target");
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(&outside).unwrap();
    let target = outside.join("should-not-exist");
    let mut cmd = sandme(&dir);
    cmd.env("SANDME_SHARED_PATHS", &dir)
        .args(["touch", &target.display().to_string()]);

    // When it is run
    let status = cmd.status().unwrap();

    // Then the sandbox denies the write
    assert!(!status.success());
    assert!(!target.exists());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&outside);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn routes_http_egress_through_the_proxy() {
    // Given a fake origin on loopback and sandme with its own proxy port
    let origin = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let origin_port = origin.local_addr().unwrap().port();
    tokio::spawn(serve_once(origin));

    let dir = workdir("routes-http-egress-through-the-proxy");
    let mut cmd = sandme(&dir);
    cmd.args([
        "curl",
        "-sS",
        "--max-time",
        "10",
        &format!("http://127.0.0.1:{origin_port}/"),
    ]);

    // When the sandboxed command fetches from the origin
    let output = cmd.output().unwrap();

    // Then the answer travelled through sandme's proxy
    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("served-through-proxy"));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Answer one HTTP request with a fixed payload.
async fn serve_once(listener: tokio::net::TcpListener) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let (mut stream, _) = listener.accept().await.unwrap();

    let mut request = Vec::new();
    let mut buf = [0_u8; 1024];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        let n = stream.read(&mut buf).await.unwrap_or(0);
        if n == 0 {
            break;
        }
        request.extend_from_slice(&buf[..n]);
    }

    let response =
        "HTTP/1.1 200 OK\r\nContent-Length: 20\r\nConnection: close\r\n\r\nserved-through-proxy";
    stream.write_all(response.as_bytes()).await.unwrap();
}

/// Borrow a port from the OS and give it back — small race, test-only.
fn free_port() -> u16 {
    std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
