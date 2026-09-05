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

/// The sandme binary under test, isolated from the developer's real config.
///
/// `SANDME_PROXY_PORT=0` lets the OS pick the port at bind time, so parallel
/// tests cannot collide: there is no window between choosing a port and
/// binding it for another test to slip into. Only a test that has to know the
/// port up front needs `free_port`.
fn sandme(workdir: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sandme"));
    cmd.env("HOME", workdir)
        .env("SANDME_PROXY_PORT", "0")
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

    // When the command is started
    // Then the proxy port is listening while the command runs (FR-005)
    let (mut child, port) = start_with_live_proxy(&dir, &["sleep", "2"]);

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

/// Start sandme on a port this test can probe, and return once its proxy
/// answers there.
///
/// This is the one case that cannot use `SANDME_PROXY_PORT=0`: the test has
/// to know the port to check that the proxy stops. `free_port` can lose its
/// port to another process before sandme binds it, which shows up as sandme
/// exiting 1 before the child ever runs — so a lost race is retried with a
/// fresh port rather than failing a test that is about proxy lifetime.
fn start_with_live_proxy(dir: &Path, args: &[&str]) -> (std::process::Child, u16) {
    for _ in 0..5 {
        let port = free_port();
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_sandme"));
        cmd.env("HOME", dir)
            .env("SANDME_PROXY_PORT", port.to_string())
            .env_remove("SANDME_SHARED_PATHS")
            .current_dir(dir)
            .args(args);
        let mut child = cmd.spawn().unwrap();

        // Poll rather than sleep a fixed span, so a loaded machine is slow
        // here instead of flaky.
        let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            if std::net::TcpStream::connect(addr).is_ok() {
                return (child, port);
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        let _ = child.kill();
        let _ = child.wait();
    }
    panic!("sandme's proxy never came up: five ports in a row were taken");
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

#[test]
fn single_string_command_runs_through_shell() {
    // Given a single quoted string with arguments
    let dir = workdir("single-string-command-runs-through-shell");
    let mut cmd = sandme(&dir);
    cmd.arg("echo sandme-shell-works");

    // When it is run
    let output = cmd.output().unwrap();

    // Then the shell interpreted it and the output passes through
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "sandme-shell-works"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn single_string_command_supports_pipes() {
    // Given a single quoted string with a pipe
    let dir = workdir("single-string-command-supports-pipes");
    let mut cmd = sandme(&dir);
    cmd.arg("echo hello-world | tr '-' ' '");

    // When it is run
    let output = cmd.output().unwrap();

    // Then the pipe was interpreted by the shell
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "hello world"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn single_string_command_supports_redirection() {
    // Given a single quoted string with output redirection
    let dir = workdir("single-string-command-supports-redirection");
    let output_file = dir.join("redirected.txt");
    let mut cmd = sandme(&dir);
    cmd.env("SANDME_SHARED_PATHS", &dir).arg(format!(
        "echo redirected-content > {}",
        output_file.display()
    ));

    // When it is run
    let status = cmd.status().unwrap();

    // Then the redirection worked and the file was created
    assert!(status.success());
    assert!(output_file.exists());
    assert_eq!(
        std::fs::read_to_string(&output_file).unwrap().trim_end(),
        "redirected-content"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn single_string_command_propagates_exit_code() {
    // Given a single quoted string that exits with a specific code
    let dir = workdir("single-string-command-propagates-exit-code");
    let mut cmd = sandme(&dir);
    cmd.arg("exit 42");

    // When it is run
    let status = cmd.status().unwrap();

    // Then the exit code propagates
    assert_eq!(status.code(), Some(42));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn multi_arg_command_still_works_directly() {
    // Given multiple arguments (already split by the caller's shell)
    let dir = workdir("multi-arg-command-still-works-directly");
    let mut cmd = sandme(&dir);
    cmd.args(["echo", "multi-arg-works"]);

    // When it is run
    let output = cmd.output().unwrap();

    // Then direct exec still works (no shell involved)
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "multi-arg-works"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn runs_shell_process_substitution() {
    // Given a shell command that uses process substitution, which bash
    // implements by handing the child a /dev/fd/N path
    let dir = workdir("runs-shell-process-substitution");
    let mut cmd = sandme(&dir);
    cmd.args(["/bin/bash", "-c", "cat <(echo procsub-works)"]);

    // When it is run
    let output = cmd.output().unwrap();

    // Then the substituted output reaches the command, with no denial on stderr
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "procsub-works"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Operation not permitted"),
        "the sandbox denied a /dev/fd path: {stderr:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn writes_to_the_standard_stream_devices() {
    // Given a shell command that writes through /dev/stdout and /dev/stderr
    let dir = workdir("writes-to-the-standard-stream-devices");
    let mut cmd = sandme(&dir);
    cmd.args([
        "/bin/sh",
        "-c",
        "echo out-works > /dev/stdout; echo err-works > /dev/stderr",
    ]);

    // When it is run
    let output = cmd.output().unwrap();

    // Then both devices are writable and carry the text to the right stream
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "out-works"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("err-works"),
        "stderr should carry the text written to /dev/stderr"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reads_the_random_devices() {
    // Given a command that reads from /dev/urandom
    let dir = workdir("reads-the-random-devices");
    let mut cmd = sandme(&dir);
    cmd.args(["/bin/sh", "-c", "head -c 8 /dev/urandom | wc -c"]);

    // When it is run
    let output = cmd.output().unwrap();

    // Then the read succeeds and returns the bytes asked for
    assert!(output.status.success());
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "8");
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
        .env("SANDME_PROXY_PORT", "0")
        .env_remove("SANDME_SHARED_PATHS")
        .current_dir(&dir)
        .args(["touch", &shared.join("proof").display().to_string()]);

    let status = cmd.status().unwrap();

    // Then the configured shared path was accessible
    assert!(status.success());
    assert!(shared.join("proof").exists());
    let _ = std::fs::remove_dir_all(&dir);
}

/// Borrow a port from the OS and give it back.
///
/// Inherently racy — another process can take the port before the caller
/// binds it — so this is only for a test that must know the port in advance.
/// Everything else passes `SANDME_PROXY_PORT=0` and lets sandme bind directly.
fn free_port() -> u16 {
    std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}
