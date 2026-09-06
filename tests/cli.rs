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
        .env_remove("SANDME_GUI_MODE")
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

/// A scratch home whose path carries no symlink.
///
/// `workdir` reaches `$TMPDIR` through `/var`, which the kernel resolves to
/// `/private/var`. A test that needs a *grant* to be the only thing standing
/// between it and the file has to name the resolved form, or the grant misses
/// and the test passes for the wrong reason.
fn canonical_workdir(name: &str) -> PathBuf {
    std::fs::canonicalize(workdir(name)).unwrap()
}

/// A `~/Library` populated the way a real one is, so a denial is proven by a
/// refusal rather than by the path simply not being there.
fn home_library(dir: &Path) -> PathBuf {
    let library = dir.join("Library");
    std::fs::create_dir_all(library.join("LaunchAgents")).unwrap();
    std::fs::create_dir_all(library.join("Keychains")).unwrap();
    std::fs::create_dir_all(library.join("Application Support")).unwrap();
    std::fs::write(
        library.join("Keychains").join("login.keychain-db"),
        "keychain-bytes",
    )
    .unwrap();
    library
}

#[test]
fn denies_writing_a_launch_agent_even_when_the_whole_home_is_shared() {
    // Given the widest configuration sandme offers — the default share of the
    // whole home directory, plus GUI mode — and a real ~/Library/LaunchAgents.
    // The home is reached through `/var`, so the denial only bites if it was
    // built from the path the kernel resolves.
    let dir = workdir("denies-writing-a-launch-agent");
    let plist = home_library(&dir).join("LaunchAgents").join("probe.plist");
    let mut cmd = sandme(&dir);
    cmd.env("SANDME_SHARED_PATHS", "~/")
        .env("SANDME_GUI_MODE", "1")
        .args(["touch", &plist.display().to_string()]);

    // When the sandboxed command tries to plant a launchd job
    let status = cmd.status().unwrap();

    // Then the sandbox refuses: a plist here would run outside the sandbox at
    // the next login, so no configuration may grant it (issue #12)
    assert!(!status.success());
    assert!(!plist.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn denies_reading_the_keychains_even_when_the_whole_home_is_shared() {
    // Given the whole home shared, GUI mode on, and a keychain to read
    let dir = workdir("denies-reading-the-keychains");
    let keychain = home_library(&dir)
        .join("Keychains")
        .join("login.keychain-db");
    let mut cmd = sandme(&dir);
    cmd.env("SANDME_SHARED_PATHS", "~/")
        .env("SANDME_GUI_MODE", "1")
        .args(["cat", &keychain.display().to_string()]);

    // When the sandboxed command tries to read it
    let output = cmd.output().unwrap();

    // Then the sandbox refuses, and none of the keychain reaches the command
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains("keychain-bytes"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn denies_the_home_library_when_gui_mode_is_off() {
    // Given a narrow share that does not include ~/Library, and no GUI mode.
    // The home is named in resolved form so that the grant this test proves
    // absent would have matched had it been emitted.
    let dir = canonical_workdir("denies-the-home-library-without-gui-mode");
    let library = home_library(&dir);
    let scratch = dir.join("scratch");
    std::fs::create_dir_all(&scratch).unwrap();
    let target = library.join("Application Support").join("probe.txt");
    let mut cmd = sandme(&dir);
    cmd.env("SANDME_SHARED_PATHS", scratch.display().to_string())
        .args(["touch", &target.display().to_string()]);

    // When the sandboxed command tries to write its state there
    let status = cmd.status().unwrap();

    // Then the sandbox refuses: ~/Library is granted for GUI mode, not always
    assert!(!status.success());
    assert!(!target.exists());
    let _ = std::fs::remove_dir_all(&dir);
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

/// Build a stand-in macOS app bundle and put its CLI wrapper on a `PATH`
/// directory of its own; returns the value to use as `PATH`.
///
/// The bundle imitates the shape every macOS IDE ships — a wrapper
/// (`Contents/MacOS/cli`, symlinked onto `PATH` the way `/usr/local/bin/zed`
/// is) next to the real executable the `Info.plist` names. A fake keeps these
/// tests runnable on a machine that has no such IDE installed: what is under
/// test is sandme's redirect, not the IDE.
///
/// `executable_name` is what the bundle's `Info.plist` names; `None` writes no
/// `Info.plist` at all, standing in for a bundle sandme cannot read through.
fn fake_app_bundle(dir: &Path, executable_name: Option<&str>) -> String {
    let macos = dir.join("Fake.app/Contents/MacOS");
    std::fs::create_dir_all(&macos).unwrap();
    write_script(&macos.join("cli"), "echo wrapper-ran");
    write_script(&macos.join("fake-main"), "echo main-executable-ran \"$@\"");

    if let Some(name) = executable_name {
        std::fs::write(
            dir.join("Fake.app/Contents/Info.plist"),
            format!(
                "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
                 <plist version=\"1.0\"><dict>\n\
                 \t<key>CFBundleExecutable</key>\n\t<string>{name}</string>\n\
                 </dict></plist>\n"
            ),
        )
        .unwrap();
    }

    let bin = dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::os::unix::fs::symlink(macos.join("cli"), bin.join("fakeapp")).unwrap();

    format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", bin.display())
}

/// Write an executable `/bin/sh` script.
fn write_script(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::write(path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn runs_the_bundle_executable_for_a_bare_command_name() {
    // Given an app bundle whose CLI wrapper is on PATH under a bare name
    let dir = workdir("runs-bundle-executable-for-bare-name");
    let path = fake_app_bundle(&dir, Some("fake-main"));
    let mut cmd = sandme(&dir);
    cmd.env("PATH", path).args(["fakeapp", "a-project"]);

    // When the user names it the way the spec's Story 1 does — `sandme zed ~/Workspace/`
    let output = cmd.output().unwrap();

    // Then the bundle's own executable ran, not the LaunchServices wrapper
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "main-executable-ran a-project",
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn runs_the_bundle_executable_for_a_quoted_command() {
    // Given the same bundle, named inside a single quoted operand
    let dir = workdir("runs-bundle-executable-for-quoted-command");
    let path = fake_app_bundle(&dir, Some("fake-main"));
    let mut cmd = sandme(&dir);
    cmd.env("PATH", path).arg("fakeapp ~/a-project");

    // When the user types the form the README documents
    let output = cmd.output().unwrap();

    // Then the bundle's executable ran, and the shell still expanded the
    // arguments it was handed
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stdout = stdout.trim_end();
    assert!(
        stdout.starts_with("main-executable-ran /") && stdout.ends_with("/a-project"),
        "expected the bundle executable with an expanded path; got {stdout:?}, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reports_an_app_bundle_it_cannot_read() {
    // Given a CLI wrapper inside a bundle with no readable Info.plist
    let dir = workdir("reports-an-app-bundle-it-cannot-read");
    let path = fake_app_bundle(&dir, None);
    let mut cmd = sandme(&dir);
    cmd.env("PATH", path).arg("fakeapp");

    // When it is run
    let output = cmd.output().unwrap();

    // Then sandme says on stderr that it recognised the bundle and gave up,
    // instead of leaving the user with only the app's own failure
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("sandme: fakeapp") && stderr.contains("Fake.app"),
        "expected a diagnostic naming the bundle; got {stderr:?}"
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "wrapper-ran"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn leaves_a_compound_shell_command_to_the_shell() {
    // Given a quoted command whose first word is not the program
    let dir = workdir("leaves-a-compound-shell-command-to-the-shell");
    let path = fake_app_bundle(&dir, Some("fake-main"));
    let mut cmd = sandme(&dir);
    cmd.env("PATH", path).arg("true && fakeapp");

    // When it is run
    let output = cmd.output().unwrap();

    // Then sandme rewrote nothing — guessing which word of a compound command
    // is the program would risk running something the user never asked for
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "wrapper-ran"
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
