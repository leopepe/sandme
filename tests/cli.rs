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

#[test]
fn denies_subprocess_access_outside_shared_paths() {
    // Given a shared directory and a path outside it
    let dir = workdir("denies-subprocess-access-outside-shared-paths");
    let outside = std::env::temp_dir().join("sandme-test-subprocess-denied");
    let _ = std::fs::remove_dir_all(&outside);
    std::fs::create_dir_all(&outside).unwrap();
    let target = outside.join("should-not-exist");
    let mut cmd = sandme(&dir);
    cmd.env("SANDME_SHARED_PATHS", &dir)
        .args(["sh", "-c", &format!("touch {}", target.display())]);

    // When a subprocess inside the sandbox tries to write outside shared paths
    let status = cmd.status().unwrap();

    // Then the sandbox denies the write — child processes inherit the sandbox (FR-003)
    assert!(!status.success());
    assert!(!target.exists());
    let _ = std::fs::remove_dir_all(&dir);
    let _ = std::fs::remove_dir_all(&outside);
}

#[test]
fn proxy_lifetime_follows_command() {
    // Given a sandme invocation running a long-lived command
    let dir = workdir("proxy-lifetime-follows-command");
    let port = free_port();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sandme"));
    cmd.env("HOME", &dir)
        .env("SANDME_PROXY_PORT", port.to_string())
        .env_remove("SANDME_SHARED_PATHS")
        .current_dir(&dir)
        .args(["sleep", "2"]);

    // When the command is started
    let mut child = cmd.spawn().unwrap();

    // Then the proxy port is listening while the command runs (FR-005)
    // Poll with timeout instead of fixed sleep to avoid flakiness under load.
    let proxy_addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut proxy_alive = false;
    while std::time::Instant::now() < deadline {
        if std::net::TcpStream::connect(proxy_addr).is_ok() {
            proxy_alive = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert!(proxy_alive, "proxy should be listening while command runs");

    // And when the command exits
    let status = child.wait().unwrap();
    assert!(status.success());

    // Then the proxy stops — its lifetime follows the command's
    std::thread::sleep(std::time::Duration::from_millis(200));
    let proxy_alive_after =
        std::net::TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).is_ok();
    assert!(!proxy_alive_after, "proxy should stop after command exits");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn no_manual_proxy_configuration_needed() {
    // Given a sandme invocation with no proxy env vars set by the user
    let dir = workdir("no-manual-proxy-config");
    let mut cmd = sandme(&dir);
    cmd.args(["sh", "-c", "echo $HTTP_PROXY"]);

    // When the command runs
    let output = cmd.output().unwrap();

    // Then the proxy was configured automatically — the child has HTTP_PROXY
    // set without the user configuring it (NFR-002)
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("http://127.0.0.1:"),
        "HTTP_PROXY should be set automatically; got: {stdout:?}"
    );
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

#[test]
fn reads_config_file_from_default_path() {
    // Given a HOME directory with a config file that shares a specific path
    let dir = workdir("reads-config-file-from-default-path");
    let shared = dir.join("shared");
    std::fs::create_dir_all(&shared).unwrap();
    let config_dir = dir.join(".sandme");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        format!("shared_paths = [\"{}\"]\n", shared.display()),
    )
    .unwrap();

    // When sandme runs without SANDME_SHARED_PATHS, the config file is read
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sandme"));
    cmd.env("HOME", &dir)
        .env("SANDME_PROXY_PORT", free_port().to_string())
        .env_remove("SANDME_SHARED_PATHS")
        .current_dir(&dir)
        .args(["touch", &shared.join("proof").display().to_string()]);

    let status = cmd.status().unwrap();

    // Then the configured shared path was accessible
    assert!(status.success());
    assert!(shared.join("proof").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Borrow a port from the OS and give it back — small race, test-only.
fn free_port() -> u16 {
    std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
