//! Integration tests: the built binary, driven through its CLI (SPEC-0001).

use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr};
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
    sandme_at(workdir, workdir)
}

/// The sandme binary with its home directory and working directory set apart.
///
/// The default share is the working directory (SPEC-0007 FR-701), so a test
/// that must tell a working-directory grant from a home-directory one needs the
/// two to be different places. `sandme` above is the common case where they
/// coincide.
fn sandme_at(home: &Path, cwd: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sandme"));
    cmd.env("HOME", home)
        .env("SANDME_PROXY_PORT", "0")
        .env_remove("SANDME_SHARED_PATHS")
        .env_remove("SANDME_GUI_MODE")
        .env_remove("SANDME_ALLOW_PRIVATE_EGRESS")
        .current_dir(cwd);
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
    // The origin is on loopback, which the proxy refuses by default
    // (SPEC-0003 FR-201), so this test opts out to keep its subject the
    // routing rather than the destination policy.
    cmd.env("SANDME_ALLOW_PRIVATE_EGRESS", "1");
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
fn confines_to_the_working_directory_by_default() {
    // Given a working directory and a separate home directory, with nothing
    // configured — the out-of-the-box posture
    let home = workdir("confines-to-cwd-home");
    let cwd = workdir("confines-to-cwd-project");
    let target = cwd.join("proof");
    let mut cmd = sandme_at(&home, &cwd);
    cmd.args(["touch", &target.display().to_string()]);

    // When the sandboxed command writes inside the working directory
    let status = cmd.status().unwrap();

    // Then the write succeeds: the working directory is the default share,
    // so a tool run in a project reaches that project with no configuration
    // (SPEC-0007 FR-701)
    assert!(status.success());
    assert!(target.exists());
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn denies_the_home_outside_the_working_directory_by_default() {
    // Given the same default posture, a home directory apart from the working
    // directory, and a ~/.ssh to write into
    let home = canonical_workdir("denies-home-default-home");
    let cwd = workdir("denies-home-default-project");
    std::fs::create_dir_all(home.join(".ssh")).unwrap();
    let target = home.join(".ssh").join("evil");
    let mut cmd = sandme_at(&home, &cwd);
    cmd.args(["touch", &target.display().to_string()]);

    // When the sandboxed command writes outside the working directory
    let status = cmd.status().unwrap();

    // Then the sandbox refuses: the default no longer shares the whole home,
    // so ~/.ssh, ~/.aws and shell history are not exposed out of the box
    // (SPEC-0007 FR-701, issue #12 §2)
    assert!(!status.success());
    assert!(!target.exists());
    let _ = std::fs::remove_dir_all(&home);
    let _ = std::fs::remove_dir_all(&cwd);
}

#[test]
fn denies_the_world_temp_directory_under_gui_mode() {
    // Given GUI mode on, a scratch share, and a target directly under the
    // world-shared /private/tmp
    let dir = workdir("denies-world-temp-under-gui");
    let target = std::path::Path::new("/private/tmp").join("sandme-test-worldtmp-probe");
    let _ = std::fs::remove_file(&target);
    let mut cmd = sandme(&dir);
    cmd.env("SANDME_GUI_MODE", "1")
        .env("SANDME_SHARED_PATHS", &dir)
        .args(["touch", &target.display().to_string()]);

    // When the sandboxed command tries to write there
    let status = cmd.status().unwrap();

    // Then the sandbox refuses: GUI mode grants the per-user $TMPDIR, not all
    // of /private/tmp — a surface shared with unsandboxed processes
    // (SPEC-0007 FR-702, issue #12 §3)
    assert!(!status.success());
    assert!(!target.exists());
    let _ = std::fs::remove_file(&target);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn allows_a_git_style_temp_write_under_gui_mode() {
    // Given GUI mode on and a scratch share. The child inherits $TMPDIR, so it
    // writes to the same per-user temp directory git's xcrun shim uses (#29).
    let dir = workdir("allows-git-style-temp-under-gui");
    let mut cmd = sandme(&dir);
    cmd.env("SANDME_GUI_MODE", "1")
        .env("SANDME_SHARED_PATHS", &dir)
        .args([
            "sh",
            "-c",
            "f=\"$TMPDIR/sandme-test-gittemp-probe\"; touch \"$f\" && rm -f \"$f\" && echo OK",
        ]);

    // When the sandboxed command writes scratch under $TMPDIR
    let output = cmd.output().unwrap();

    // Then it succeeds: narrowing the temp grant kept the per-user temp dir
    // writable, so editors and git-style temp writes still work (SPEC-0007
    // NFR-701)
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim_end(), "OK");
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
    // set without the user configuring it (NFR-002), and it carries this
    // invocation's credential, so the child needs no configuration to
    // authenticate either (SPEC-0003 FR-203)
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("http://sandme:") && stdout.contains("@127.0.0.1:"),
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

#[test]
fn allocates_a_pseudo_terminal() {
    // Given a command that allocates a pseudo-terminal. `/usr/bin/script`
    // opens one with openpty(3) — the same path Zed's integrated terminal and
    // its login-shell environment loading take, both of which failed with
    // "out of pty devices" until the sandbox permitted PTY allocation
    // (issue #29). It lives under /usr, so it needs no shared path of its own.
    let dir = workdir("allocates-a-pseudo-terminal");
    let mut cmd = sandme(&dir);
    cmd.args([
        "/usr/bin/script",
        "-q",
        "/dev/null",
        "/bin/echo",
        "pty-works",
    ]);

    // When it is run
    let output = cmd.output().unwrap();

    // Then the pseudo-terminal is allocated: the command runs on it instead of
    // dying with "openpty: Operation not permitted"
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "the sandbox denied PTY allocation; stderr: {stderr:?}"
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("pty-works"),
        "the command should have run on the allocated PTY; stderr: {stderr:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn controls_a_pseudo_terminal() {
    // Given a command that does what a terminal emulator does — allocate a PTY,
    // then claim the slave as its controlling terminal with `ioctl(TIOCSCTTY)`.
    // Zed's integrated terminal (via portable-pty) fails at exactly this step
    // with "Failed to spawn … Operation not permitted" unless the profile
    // grants `file-ioctl` on the slave (issue #29).
    let dir = workdir("controls-a-pseudo-terminal");

    // When it runs under sandme
    let output = probe_under_sandme(&dir, "SANDME_PROBE_CTTY", "1");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Then the kernel lets it claim the controlling terminal
    assert!(
        stdout.contains("PROBE-CTTY-OK"),
        "the sandbox denied TIOCSCTTY on the slave PTY: {stdout}"
    );
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

#[test]
fn reports_a_malformed_config_with_the_reserved_status() {
    // Given a config file that is not valid TOML
    let dir = workdir("reports-a-malformed-config");
    let config_dir = dir.join(".sandme");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(config_dir.join("config.toml"), "this is not toml {{{\n").unwrap();
    let mut cmd = sandme(&dir);
    cmd.args(["echo", "hi"]);

    // When sandme is run
    let output = cmd.output().unwrap();

    // Then it exits with the status reserved for its own failures, and says on
    // stderr that the failure was its own (FR-201, FR-202)
    assert_eq!(output.status.code(), Some(125));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("sandme: config file could not be parsed"),
        "expected a prefixed diagnostic naming the cause; got {stderr:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reports_a_proxy_that_cannot_bind_with_the_reserved_status() {
    // Given a port already taken by someone else
    let dir = workdir("reports-a-proxy-that-cannot-bind");
    let port = free_port();
    let holder = std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
        .expect("the port was free a moment ago");
    let mut cmd = sandme(&dir);
    cmd.env("SANDME_PROXY_PORT", port.to_string())
        .args(["echo", "hi"]);

    // When sandme is asked to put its proxy there
    let output = cmd.output().unwrap();
    drop(holder);

    // Then the invocation fails with sandme's own status, not the command's
    // (FR-201, FR-202)
    assert_eq!(output.status.code(), Some(125));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.starts_with("sandme: proxy could not listen on port"),
        "expected a prefixed diagnostic naming the port; got {stderr:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reports_a_command_it_cannot_find_as_127() {
    // Given a name nothing on PATH answers to, passed as operands
    let dir = workdir("reports-a-command-it-cannot-find");
    let mut cmd = sandme(&dir);
    cmd.args(["sandme-no-such-command", "an-argument"]);

    // When it is run
    let output = cmd.output().unwrap();

    // Then sandme reports the status every shell reports for it, and names the
    // command on stderr (FR-203)
    assert_eq!(output.status.code(), Some(127));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("sandme: sandme-no-such-command: command not found"),
        "expected a prefixed not-found diagnostic; got {stderr:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reports_a_missing_command_in_a_shell_string_as_127() {
    // Given the same name inside a single quoted operand, where /bin/sh does
    // the lookup instead of sandme
    let dir = workdir("reports-a-missing-command-in-a-shell-string");
    let mut cmd = sandme(&dir);
    cmd.arg("sandme-no-such-command an-argument");

    // When it is run
    let status = cmd.status().unwrap();

    // Then the two forms agree: the same command is the same status (FR-203)
    assert_eq!(status.code(), Some(127));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reports_a_command_that_is_not_executable_as_126() {
    // Given a file that exists and carries no execute bit
    let dir = workdir("reports-a-command-that-is-not-executable");
    let script = dir.join("not-executable.sh");
    std::fs::write(&script, "#!/bin/sh\necho ran\n").unwrap();
    let program = script.display().to_string();
    let mut cmd = sandme(&dir);
    cmd.args([program.as_str(), "an-argument"]);

    // When it is run
    let output = cmd.output().unwrap();

    // Then sandme distinguishes it from a command that is simply absent
    // (FR-204)
    assert_eq!(output.status.code(), Some(126));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains(&format!("sandme: {program}: found but not executable")),
        "expected a prefixed not-executable diagnostic; got {stderr:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reports_a_non_executable_command_in_a_shell_string_as_126() {
    // Given the same file, named inside a single quoted operand
    let dir = workdir("reports-a-non-executable-command-in-a-shell-string");
    let script = dir.join("not-executable.sh");
    std::fs::write(&script, "#!/bin/sh\necho ran\n").unwrap();
    let mut cmd = sandme(&dir);
    cmd.arg(script.display().to_string());

    // When it is run
    let status = cmd.status().unwrap();

    // Then the shell's answer matches sandme's own (FR-204)
    assert_eq!(status.code(), Some(126));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn propagates_a_childs_own_reserved_status() {
    // Given commands that choose the reserved statuses for themselves
    let dir = workdir("propagates-a-childs-own-reserved-status");

    for code in [125, 126, 127] {
        // When each is run under sandme
        let mut cmd = sandme(&dir);
        cmd.args(["sh", "-c", &format!("exit {code}")]);
        let output = cmd.output().unwrap();

        // Then the status is passed through untouched: reserving a value binds
        // sandme, not the command it wraps (FR-205)
        assert_eq!(output.status.code(), Some(code));
        assert!(
            output.stderr.is_empty(),
            "sandme spoke about a status it only forwarded: {:?}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn propagates_an_unexplained_exec_status() {
    // Given a command that runs and chooses 71 — the status sandbox-exec uses
    // for an exec failure — for itself
    let dir = workdir("propagates-an-unexplained-exec-status");
    let mut cmd = sandme(&dir);
    cmd.args(["sh", "-c", "exit 71"]);

    // When it is run
    let output = cmd.output().unwrap();

    // Then sandme leaves it alone: it reinterprets 71 only when it can show
    // the command could not have run at all (FR-205)
    assert_eq!(output.status.code(), Some(71));
    assert!(
        output.stderr.is_empty(),
        "sandme explained a status it could not explain: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// SPEC-0003 — the proxy refuses the destinations the sandbox denies the
// command directly, and relays only for the invocation that started it.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refuses_to_relay_to_a_service_on_host_loopback() {
    // Given a host-only service on loopback, and a sandboxed command asking
    // the proxy for it — the pivot reported in issue #15
    let origin = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let origin_port = origin.local_addr().unwrap().port();
    tokio::spawn(serve_once(origin));

    let dir = workdir("refuses-to-relay-to-host-loopback");
    let mut cmd = sandme(&dir);
    cmd.args([
        "curl",
        "-sS",
        "--max-time",
        "10",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        &format!("http://127.0.0.1:{origin_port}/"),
    ]);

    // When the sandboxed command runs
    let output = cmd.output().unwrap();

    // Then the proxy answers 403, the service's content never reaches the
    // command, and stderr names the setting that would permit it (FR-202)
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stdout.trim(), "403", "stderr was: {stderr}");
    assert!(!stdout.contains("served-through-proxy"));
    assert!(
        stderr.contains("proxy refused") && stderr.contains("allow_private_egress"),
        "stderr should say what to change; got: {stderr}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn relays_to_host_loopback_when_private_egress_is_allowed() {
    // Given the opt-out enabled — the local-model-server workflow — and an
    // origin that echoes back the request it received
    let origin = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let origin_port = origin.local_addr().unwrap().port();
    tokio::spawn(echo_request_once(origin));

    let dir = workdir("relays-to-host-loopback-when-allowed");
    let mut cmd = sandme(&dir);
    cmd.env("SANDME_ALLOW_PRIVATE_EGRESS", "1").args([
        "curl",
        "-sS",
        "--max-time",
        "10",
        &format!("http://127.0.0.1:{origin_port}/"),
    ]);

    // When the sandboxed command runs
    let output = cmd.output().unwrap();

    // Then the request is relayed, and the credential is not relayed with it:
    // it authenticates the child to sandme and is no origin's business (FR-204)
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("GET / HTTP/1.1"), "got: {stdout:?}");
    assert!(
        !stdout.to_lowercase().contains("proxy-authorization"),
        "the origin should never see the credential; got: {stdout:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn refuses_to_relay_to_the_cloud_metadata_address() {
    // Given a sandboxed command asking the proxy for the link-local cloud
    // metadata endpoint
    let dir = workdir("refuses-to-relay-to-metadata");
    let mut cmd = sandme(&dir);
    cmd.args([
        "curl",
        "-sS",
        "--max-time",
        "10",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "http://169.254.169.254/latest/meta-data/",
    ]);

    // When it runs
    let started = std::time::Instant::now();
    let output = cmd.output().unwrap();
    let elapsed = started.elapsed();

    // Then the proxy answers 403, and answers it without having tried to
    // connect — a connection attempt to a link-local address hangs until it
    // times out, so a prompt answer is the observable form of NFR-202
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "403");
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "the refusal should precede any connection attempt; took {elapsed:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn refuses_a_local_client_without_the_invocation_credential() {
    use std::io::{Read, Write};

    // Given a sandme invocation in flight, and an unrelated local process —
    // this test — talking to its proxy port with no credential
    let dir = workdir("refuses-a-local-client-without-credential");
    let (mut child, port) = start_with_live_proxy(&dir, &["sleep", "5"]);

    // When it sends a plain request and a CONNECT, as any local process could
    let answer = |request: &str| {
        let mut stream =
            std::net::TcpStream::connect(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut answer = String::new();
        let _ = stream.read_to_string(&mut answer);
        answer
    };
    let forwarded = answer(
        "GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n",
    );
    let tunnelled = answer(
        "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nConnection: close\r\n\r\n",
    );

    let _ = child.kill();
    let _ = child.wait();

    // Then both are refused with 407 and nothing is relayed: the proxy is the
    // child's, not the machine's (FR-203)
    assert!(forwarded.starts_with("HTTP/1.1 407"), "got: {forwarded:?}");
    assert!(tunnelled.starts_with("HTTP/1.1 407"), "got: {tunnelled:?}");
    assert!(
        forwarded
            .to_lowercase()
            .contains("proxy-authenticate: basic"),
        "the refusal should carry the challenge; got: {forwarded:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn refuses_a_local_client_on_the_ipv6_listener_without_the_invocation_credential() {
    use std::io::{Read, Write};

    // Given a sandme invocation in flight, and an unrelated local process
    // reaching the proxy over IPv6 loopback — the second listener, which #10
    // item 2 reported once bound a port of its own and went unchecked
    let dir = workdir("refuses-ipv6-client-without-credential");
    let (mut child, port) = start_with_live_proxy(&dir, &["sleep", "5"]);

    // When it sends a plain request and a CONNECT to [::1]:port with no
    // credential, as any local process could
    let answer = |request: &str| {
        let mut stream =
            std::net::TcpStream::connect(SocketAddr::from((Ipv6Addr::LOCALHOST, port))).unwrap();
        stream.write_all(request.as_bytes()).unwrap();
        let mut answer = String::new();
        let _ = stream.read_to_string(&mut answer);
        answer
    };
    let forwarded = answer(
        "GET http://example.com/ HTTP/1.1\r\nHost: example.com\r\nConnection: close\r\n\r\n",
    );
    let tunnelled = answer(
        "CONNECT example.com:443 HTTP/1.1\r\nHost: example.com:443\r\nConnection: close\r\n\r\n",
    );

    let _ = child.kill();
    let _ = child.wait();

    // Then the IPv6 listener refuses both with 407, exactly as the IPv4 one
    // does: the credential gate is not bound to one address family (FR-203)
    assert!(forwarded.starts_with("HTTP/1.1 407"), "got: {forwarded:?}");
    assert!(tunnelled.starts_with("HTTP/1.1 407"), "got: {tunnelled:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn refuses_to_relay_to_host_loopback_over_the_ipv6_listener() {
    // Given a host-only service on loopback, and a sandboxed command that
    // reaches the proxy over IPv6 loopback instead of IPv4 — so both the
    // credential the child presents and the destination restriction are
    // exercised on the [::1] listener, not only the IPv4 one (FR-201, FR-203)
    let origin = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let origin_port = origin.local_addr().unwrap().port();
    tokio::spawn(serve_once(origin));

    let dir = workdir("refuses-ipv6-pivot-to-host-loopback");
    let mut cmd = sandme(&dir);
    // The child rewrites its own proxy URL from 127.0.0.1 to [::1], reusing the
    // credential sandme put in HTTP_PROXY. The request therefore lands on the
    // IPv6 listener while still authenticating as this invocation's child, so a
    // 403 (not a 407) proves the IPv6 path accepted the credential and then
    // refused the destination.
    cmd.arg(format!(
        "curl -sS --max-time 10 -o /dev/null -w '%{{http_code}}' \
         -x \"$(printf %s \"$HTTP_PROXY\" | sed s/127.0.0.1/[::1]/)\" \
         http://127.0.0.1:{origin_port}/"
    ));

    // When the sandboxed command runs
    let output = cmd.output().unwrap();

    // Then the IPv6 listener answers 403 and the host-only content never
    // reaches the command: the pivot is closed on both loopback families
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert_eq!(stdout.trim(), "403", "stderr was: {stderr}");
    assert!(!stdout.contains("served-through-proxy"));
    let _ = std::fs::remove_dir_all(&dir);
}

// SPEC-0011 — proxy hardening follow-ups from the audit (issue #32).

#[test]
fn refuses_to_relay_to_a_cgnat_destination_by_default() {
    // Given a sandboxed command asking the proxy for a carrier-grade NAT
    // address — the range Tailscale assigns tailnet peers from, which the proxy
    // relayed to on defaults before this fix (issue #32 §3)
    let dir = workdir("refuses-to-relay-to-cgnat");
    let mut cmd = sandme(&dir);
    cmd.args([
        "curl",
        "-sS",
        "--max-time",
        "10",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "http://100.64.0.1/",
    ]);

    // When it runs
    let started = std::time::Instant::now();
    let output = cmd.output().unwrap();
    let elapsed = started.elapsed();

    // Then the proxy answers 403 without having tried to connect — a prompt
    // answer is the observable form of "no connection attempt" (FR-1104)
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "403");
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "the refusal should precede any connection attempt; took {elapsed:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn refuses_to_relay_to_a_benchmarking_destination_by_default() {
    // Given a sandboxed command asking the proxy for a benchmarking address
    // (198.18.0.0/15, RFC 2544), also relayed on defaults before this fix
    let dir = workdir("refuses-to-relay-to-benchmarking");
    let mut cmd = sandme(&dir);
    cmd.args([
        "curl",
        "-sS",
        "--max-time",
        "10",
        "-o",
        "/dev/null",
        "-w",
        "%{http_code}",
        "http://198.18.0.1/",
    ]);

    // When it runs
    let started = std::time::Instant::now();
    let output = cmd.output().unwrap();
    let elapsed = started.elapsed();

    // Then the proxy answers 403 before any connection attempt (FR-1104)
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim(), "403");
    assert!(
        elapsed < std::time::Duration::from_secs(5),
        "the refusal should precede any connection attempt; took {elapsed:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn strips_hop_by_hop_headers_and_pins_host_to_the_vetted_authority() {
    // Given an origin that echoes the request it received, reached through the
    // proxy with private egress allowed (the origin is on loopback), and a
    // client that sends a hop-by-hop Proxy-Connection header and a Host of its
    // own choosing (issue #32 §4)
    let origin = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let origin_port = origin.local_addr().unwrap().port();
    tokio::spawn(echo_request_once(origin));

    let dir = workdir("strips-hop-by-hop-and-pins-host");
    let mut cmd = sandme(&dir);
    cmd.env("SANDME_ALLOW_PRIVATE_EGRESS", "1").args([
        "curl",
        "-sS",
        "--max-time",
        "10",
        "-H",
        "Proxy-Connection: Keep-Alive",
        "-H",
        "Host: internal.example",
        &format!("http://127.0.0.1:{origin_port}/"),
    ]);

    // When the sandboxed command runs
    let output = cmd.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let lower = stdout.to_lowercase();

    // Then the origin saw the authority the proxy vetted as Host, not the vhost
    // the client chose (FR-1103), and the hop-by-hop Proxy-Connection header did
    // not reach it (FR-1102)
    assert!(
        lower.contains(&format!("host: 127.0.0.1:{origin_port}")),
        "Host should be pinned to the vetted authority; got: {stdout:?}"
    );
    assert!(
        !lower.contains("internal.example"),
        "the client-chosen Host must not reach the origin; got: {stdout:?}"
    );
    assert!(
        !lower.contains("proxy-connection"),
        "hop-by-hop headers must not reach the origin; got: {stdout:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn warns_when_the_ipv6_loopback_listener_cannot_bind() {
    // Given a process squatting [::1]:port with IPv4 loopback on that port free
    // — the silent-failure condition issue #32 §2 reports
    let dir = workdir("warns-when-ipv6-listener-cannot-bind");
    let (_squatter, port) = v6_squatter_on_a_v4_free_port();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_sandme"));
    cmd.env("HOME", &dir)
        .env("SANDME_PROXY_PORT", port.to_string())
        .env_remove("SANDME_SHARED_PATHS")
        .env_remove("SANDME_ALLOW_PRIVATE_EGRESS")
        .current_dir(&dir)
        .args(["true"]);

    // When sandme starts its proxy on that port
    let output = cmd.output().unwrap();

    // Then it does not fail silently: it warns on stderr that the IPv6 loopback
    // listener could not bind, naming the address, and the child still runs —
    // the invocation continues on IPv4 (FR-1105)
    assert!(output.status.success(), "the invocation should still run");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("sandme: proxy could not also listen on IPv6 loopback")
            && stderr.contains("[::1]"),
        "expected a warning naming the IPv6 loopback listener; got: {stderr:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A held IPv6 loopback listener on a port whose IPv4 loopback is free, so a
/// sandme started there binds IPv4 and then finds IPv6 already taken.
///
/// The IPv6 bind is what stands in for the squatter of issue #32 §2; the IPv4
/// probe-bind only confirms the primary listener will come up, and is dropped
/// before it returns so sandme can take it.
fn v6_squatter_on_a_v4_free_port() -> (std::net::TcpListener, u16) {
    for _ in 0..10 {
        let squatter =
            std::net::TcpListener::bind(SocketAddr::from((Ipv6Addr::LOCALHOST, 0))).unwrap();
        let port = squatter.local_addr().unwrap().port();
        if std::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).is_ok() {
            return (squatter, port);
        }
    }
    panic!("no port free on IPv4 loopback while squatted on IPv6 loopback");
}

/// Answer one HTTP request with the request itself, so a test can assert on
/// the headers the origin received.
async fn echo_request_once(listener: tokio::net::TcpListener) {
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

    let body = String::from_utf8_lossy(&request).to_string();
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(response.as_bytes()).await.unwrap();
}

#[test]
fn denies_rewriting_sandmes_own_config_even_when_the_whole_home_is_shared() {
    // Given the widest configuration sandme offers — the default share of the
    // whole home directory, plus GUI mode — and an existing config file
    let dir = workdir("denies-rewriting-its-own-config");
    let config_dir = dir.join(".sandme");
    std::fs::create_dir_all(&config_dir).unwrap();
    let config = config_dir.join("config.toml");
    std::fs::write(&config, "proxy_port = 0\n").unwrap();
    let mut cmd = sandme(&dir);
    cmd.env("SANDME_SHARED_PATHS", "~/")
        .env("SANDME_GUI_MODE", "1")
        .args([
            "sh",
            "-c",
            "printf 'shared_paths = [\"/\"]\\n' > \"$HOME/.sandme/config.toml\"",
        ]);

    // When the sandboxed command tries to rewrite the policy that constrains it
    let status = cmd.status().unwrap();

    // Then the sandbox refuses, and the config it would have widened is intact.
    // The current run could not be widened either way — the profile is already
    // loaded — but the next one would start with the whole filesystem shared.
    assert!(!status.success());
    assert_eq!(
        std::fs::read_to_string(&config).unwrap(),
        "proxy_port = 0\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn warns_when_a_widening_setting_comes_from_the_environment() {
    // Given a widening setting supplied through the environment — the route a
    // previous sandboxed command's shell rc could have planted (issue #30)
    let dir = workdir("warns-when-widening-comes-from-environment");
    let mut cmd = sandme(&dir);
    cmd.env("SANDME_ALLOW_PRIVATE_EGRESS", "1")
        .args(["echo", "hi"]);

    // When sandme is run
    let output = cmd.output().unwrap();

    // Then it warns on stderr, naming the setting and that the environment
    // carried it, while stdout stays the child's alone (FR-1001, FR-1005)
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("sandme: warning:")
            && stderr.contains("allow_private_egress")
            && stderr.contains("SANDME_ALLOW_PRIVATE_EGRESS"),
        "expected a prefixed widening warning; got {stderr:?}"
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim_end(), "hi");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stays_silent_when_the_same_setting_comes_from_the_config_file() {
    // Given the identical widening value, but set in the user's own config file,
    // which the sandbox denies the child (commit 91da40a), with no env override
    let dir = workdir("stays-silent-when-setting-comes-from-config-file");
    let config_dir = dir.join(".sandme");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("config.toml"),
        "allow_private_egress = true\n",
    )
    .unwrap();
    let mut cmd = sandme(&dir);
    cmd.args(["echo", "hi"]);

    // When sandme is run
    let output = cmd.output().unwrap();

    // Then no widening warning is emitted: the user set it themselves, not a
    // sandboxed command via the environment (FR-1002)
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("allow_private_egress"),
        "a config-file value must not warn; got {stderr:?}"
    );
    assert_eq!(String::from_utf8_lossy(&output.stdout).trim_end(), "hi");
    let _ = std::fs::remove_dir_all(&dir);
}

// --- Process information and the host's environments (issue #31) ------------

unsafe extern "C" {
    /// `sysctl(2)`. Declared here rather than taken as a dependency: two calls
    /// in one test file do not justify a crate, and the platform fixes the
    /// signature.
    fn sysctl(
        name: *const i32,
        namelen: u32,
        oldp: *mut u8,
        oldlenp: *mut usize,
        newp: *const u8,
        newlen: usize,
    ) -> i32;

    /// `proc_pidpath(3)`: writes the executable path of `pid` into `buffer`.
    fn proc_pidpath(pid: i32, buffer: *mut u8, buffersize: u32) -> i32;

    /// `openpty(3)`: allocate a pseudo-terminal, returning master and slave fds.
    fn openpty(
        amaster: *mut i32,
        aslave: *mut i32,
        name: *mut u8,
        termp: *const u8,
        winp: *const u8,
    ) -> i32;

    /// `setsid(2)`: make the caller a session leader with no controlling tty,
    /// the precondition for claiming one.
    fn setsid() -> i32;

    /// `ioctl(2)` for `TIOCSCTTY`, which takes no argument (a null pointer).
    fn ioctl(fd: i32, request: u64, arg: *mut u8) -> i32;
}

/// `TIOCSCTTY` on Darwin: claim the slave as the controlling terminal. It is an
/// `ioctl` on the `/dev/ttysNNN` slave, so the profile must grant `file-ioctl`
/// on the slave for it — the step a terminal emulator fails at otherwise.
const TIOCSCTTY: u64 = 0x2000_7461;

/// `CTL_KERN`, `KERN_PROCARGS2`: the sysctl answering a pid's arguments and
/// environment. The pid is its third element, so it is reachable only as a
/// numeric MIB and no name-based profile rule matches it.
const KERN_PROCARGS2_MIB: [i32; 2] = [1, 49];

/// Room for the longest path `proc_pidpath` can return.
const PATH_BUFFER_BYTES: u32 = 4096;

/// Makes the kernel calls the tests below put under the sandbox, and prints
/// the result for the calling test to assert on.
///
/// Reads `SANDME_PROBE_PID` (a pid whose arguments and environment to read),
/// `SANDME_PROBE_CHILD` (fork, then read the child's executable path) and
/// `SANDME_PROBE_CTTY` (allocate a PTY and claim its controlling terminal).
/// Prints one line per request, beginning `PROBE-READ`/`PROBE-CTTY-OK` on
/// success and `PROBE-DENIED`/`PROBE-CTTY-DENIED` on refusal. Asserts nothing
/// itself; `#[ignore]` keeps it out of an ordinary run.
#[test]
#[ignore = "a fixture the process-information tests drive; not a test on its own"]
fn probe() {
    if let Ok(pid) = std::env::var("SANDME_PROBE_PID") {
        print_process_arguments(pid.parse().expect("the driver passes a pid"));
    }
    if std::env::var("SANDME_PROBE_CHILD").is_ok() {
        print_own_child_path();
    }
    if std::env::var("SANDME_PROBE_CTTY").is_ok() {
        print_controlling_terminal();
    }
}

/// Allocate a pseudo-terminal and claim the slave as the controlling terminal,
/// then print whether the kernel allowed the `TIOCSCTTY` ioctl.
///
/// This is the sequence a terminal emulator (Zed's integrated terminal,
/// `portable-pty`) runs, and the step that fails with `Operation not permitted`
/// unless the profile grants `file-ioctl` on the slave (issue #29). It forks so
/// the child is not already a process-group leader — the precondition `setsid`
/// needs.
fn print_controlling_terminal() {
    use std::io::Write;
    let (mut master, mut slave) = (0_i32, 0_i32);
    let opened = unsafe {
        openpty(
            &raw mut master,
            &raw mut slave,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if opened != 0 {
        println!(
            "PROBE-CTTY-DENIED openpty {}",
            std::io::Error::last_os_error()
        );
        return;
    }

    let child = unsafe { libc_fork() };
    if child == 0 {
        unsafe { setsid() };
        let claimed = unsafe { ioctl(slave, TIOCSCTTY, std::ptr::null_mut()) };
        // Flush explicitly: after `setsid` a lost stdout buffer would swallow
        // the result the parent captures.
        let mut out = std::io::stdout();
        if claimed == 0 {
            let _ = writeln!(out, "PROBE-CTTY-OK");
        } else {
            let _ = writeln!(out, "PROBE-CTTY-DENIED {}", std::io::Error::last_os_error());
        }
        let _ = out.flush();
        std::process::exit(0);
    }
}

/// Prints `pid`'s arguments and environment as text, or the kernel's refusal.
///
/// Non-printing bytes become newlines so the caller can assert on a marker
/// planted in the target's environment.
fn print_process_arguments(pid: i32) {
    let mib = [KERN_PROCARGS2_MIB[0], KERN_PROCARGS2_MIB[1], pid];
    let mut argmax = 0_usize;

    // The size query is the call the sandbox refuses, so nothing is read
    // before the refusal.
    let sized = unsafe {
        sysctl(
            mib.as_ptr(),
            3,
            std::ptr::null_mut(),
            &raw mut argmax,
            std::ptr::null(),
            0,
        )
    };
    if sized != 0 {
        println!("PROBE-DENIED {}", std::io::Error::last_os_error());
        return;
    }

    let mut buffer = vec![0_u8; argmax];
    let read = unsafe {
        sysctl(
            mib.as_ptr(),
            3,
            buffer.as_mut_ptr(),
            &raw mut argmax,
            std::ptr::null(),
            0,
        )
    };
    if read != 0 {
        println!("PROBE-DENIED {}", std::io::Error::last_os_error());
        return;
    }

    buffer.truncate(argmax);
    let text: String = buffer
        .iter()
        .map(|&byte| {
            if byte.is_ascii_graphic() || byte == b' ' {
                byte as char
            } else {
                '\n'
            }
        })
        .collect();
    println!("PROBE-READ {argmax} bytes\n{text}");
}

/// Forks, then prints the child's executable path, or the kernel's refusal.
fn print_own_child_path() {
    let child = unsafe { libc_fork() };
    if child == 0 {
        std::thread::sleep(std::time::Duration::from_secs(2));
        std::process::exit(0);
    }

    let mut buffer = [0_u8; PATH_BUFFER_BYTES as usize];
    let written = unsafe { proc_pidpath(child, buffer.as_mut_ptr(), PATH_BUFFER_BYTES) };
    if written > 0 {
        println!("PROBE-READ child path, {written} bytes");
    } else {
        println!("PROBE-DENIED {}", std::io::Error::last_os_error());
    }
}

unsafe extern "C" {
    /// `fork(2)`, named apart from the caller so the `unsafe` block reads as
    /// the process split it is.
    #[link_name = "fork"]
    fn libc_fork() -> i32;
}

/// Runs the probe under sandme with the default shares, plus the directory
/// holding the probe binary so the sandbox can execute it.
///
/// `variable` and `value` are the environment entry telling the probe what to
/// ask the kernel for. Returns the probe's captured output.
fn probe_under_sandme(dir: &Path, variable: &str, value: &str) -> std::process::Output {
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let exe_dir = exe.parent().expect("the test binary is in a directory");
    let shares = format!("{},{}", dir.display(), exe_dir.display());
    probe_under_sandme_sharing(dir, &shares, variable, value)
}

/// Runs the probe under sandme with `shares` passed as `shared_paths`
/// verbatim, so a caller can supply a value that is not a path.
fn probe_under_sandme_sharing(
    dir: &Path,
    shares: &str,
    variable: &str,
    value: &str,
) -> std::process::Output {
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let mut cmd = sandme(dir);
    cmd.env("SANDME_SHARED_PATHS", shares)
        .env(variable, value)
        .args([
            &exe.display().to_string(),
            "--ignored",
            "--exact",
            "probe",
            "--nocapture",
        ]);
    cmd.output().unwrap()
}

/// Runs the probe with no sandbox between it and the kernel, so a test can
/// establish that the read it expects to be refused works otherwise.
fn probe_unsandboxed(variable: &str, value: &str) -> std::process::Output {
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    Command::new(exe)
        .env(variable, value)
        .args(["--ignored", "--exact", "probe", "--nocapture"])
        .output()
        .unwrap()
}

/// Spawns a process outside the sandbox holding `marker` in its environment.
///
/// Not a platform binary: macOS shields those from this read whatever the
/// profile says, which would make the assertions below pass for the wrong
/// reason.
fn secret_holder(dir: &Path, marker: &str) -> std::process::Child {
    Command::new(env!("CARGO_BIN_EXE_sandme"))
        .env("HOME", dir)
        .env("SANDME_PROXY_PORT", "0")
        .env("SANDME_TEST_HOST_SECRET", marker)
        .args(["sleep", "20"])
        .spawn()
        .unwrap()
}

#[test]
fn denies_reading_another_process_environment() {
    // Given a process outside the sandbox holding a secret, which the probe
    // can read when nothing sandboxes it
    let dir = workdir("denies-reading-another-process-environment");
    let marker = "sandme-test-host-secret-4f19c7";
    let mut holder = secret_holder(&dir, marker);
    let pid = holder.id().to_string();

    let control = probe_unsandboxed("SANDME_PROBE_PID", &pid);
    let control_out = String::from_utf8_lossy(&control.stdout);
    assert!(
        control_out.contains(marker),
        "the probe cannot read the holder's environment outside the sandbox either, \
         so the assertion below would pass for the wrong reason: {control_out}"
    );

    // When a sandboxed command asks the kernel for that process's arguments
    // and environment
    let output = probe_under_sandme(&dir, "SANDME_PROBE_PID", &pid);
    let _ = holder.kill();
    let _ = holder.wait();
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Then the kernel refuses, and the secret never reaches the command
    assert!(
        stdout.contains("PROBE-DENIED"),
        "expected the read to be refused, got: {stdout}"
    );
    assert!(!stdout.contains(marker), "the environment leaked: {stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn denies_reading_another_process_environment_under_an_injected_grant() {
    // Given the same holder, and a shared path that closes the literal it is
    // interpolated into and appends the two grants this profile narrows —
    // the SBPL injection of issue #41.
    let dir = workdir("denies-reading-under-an-injected-grant");
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let exe_dir = exe.parent().expect("the test binary is in a directory");
    let injected = format!(
        "{}\")) (allow process-info-pidinfo) (allow sysctl-read) (allow file-read* (subpath \"{}",
        dir.display(),
        exe_dir.display()
    );
    let marker = "sandme-test-host-secret-b73e02";
    let mut holder = secret_holder(&dir, marker);
    let pid = holder.id().to_string();

    // When a sandboxed command asks for that process's environment
    let output = probe_under_sandme_sharing(&dir, &injected, "SANDME_PROBE_PID", &pid);
    let _ = holder.kill();
    let _ = holder.wait();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Then sandme refuses to run at all: `checked_profile_path` rejects a
    // shared path carrying a quote before the profile is built (SPEC-0007
    // FR-704), so the injection never reaches SBPL and the environment cannot
    // leak. The `(deny process-info-pidinfo)` rule that would otherwise outrank
    // such a grant (SPEC-0005 FR-305) is the defence-in-depth behind that guard.
    assert!(
        !output.status.success(),
        "sandme ran a profile built from an injected shared path: {stdout}"
    );
    assert!(
        stderr.contains("sandme:"),
        "expected sandme's own refusal, got: {stderr}"
    );
    assert!(!stdout.contains(marker), "the environment leaked: {stdout}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reads_its_own_child_process_information() {
    // Given a sandboxed command that forks
    let dir = workdir("reads-its-own-child-process-information");

    // When it asks the kernel for its child's executable path
    let output = probe_under_sandme(&dir, "SANDME_PROBE_CHILD", "1");
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Then it gets an answer: the grant covers the sandbox instance, not only
    // the calling process, so a command may still supervise what it starts
    assert!(
        stdout.contains("PROBE-READ child path"),
        "a command cannot read its own child's process information: {stdout} \
         {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn reads_a_sysctl_outside_the_denied_prefix() {
    // Given the CPU count, which every thread pool on the machine asks for
    let dir = workdir("reads-a-sysctl-outside-the-denied-prefix");
    let mut cmd = sandme(&dir);
    cmd.args(["/usr/sbin/sysctl", "hw.ncpu"]);

    // When a sandboxed command reads it
    let output = cmd.output().unwrap();

    // Then it gets a value: the denial filters one name prefix, where denying
    // the operation would take this read with it
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("hw.ncpu:"),
        "expected a value, got: {} {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn refuses_to_run_when_tmpdir_would_inject_into_the_profile() {
    // Given GUI mode and a `$TMPDIR` crafted to close the subpath literal it is
    // written into and append `(allow default)` — the escape the profile's
    // per-user temp grant would otherwise carry (issue #41, found by
    // /security-audit 2026-09-11). The share is an empty cwd, so only the
    // injected rule could grant a write to $HOME.
    let dir = workdir("refuses-a-tmpdir-injection");
    let marker = dir.join("escaped");
    let injection = "/tmp\")) (allow default) (allow file-read* (subpath \"/";
    let mut cmd = sandme_at(&dir, &dir);
    cmd.env("SANDME_GUI_MODE", "1")
        .env("TMPDIR", injection)
        .args(["sh", "-c", &format!("echo pwned > {}", marker.display())]);

    // When sandme builds the profile from that environment
    let output = cmd.output().unwrap();

    // Then it refuses to run rather than emit the injected profile, says so on
    // its own channel, and the write the injected grant would have allowed
    // never happened
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("sandme:"),
        "expected sandme's own diagnostic, got: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!marker.exists(), "the injected grant reopened the escape");
    let _ = std::fs::remove_dir_all(&dir);
}

// SPEC-0012 — git-over-SSH reaches its remote by tunnelling through the proxy
// over HTTP CONNECT, with no configuration by the user (issue #33, #29).

#[test]
fn wires_the_git_ssh_command_into_the_child() {
    // Given a sandme invocation with no GIT_SSH_COMMAND of the user's own
    let dir = workdir("wires-the-git-ssh-command");
    let mut cmd = sandme(&dir);
    cmd.env_remove("GIT_SSH_COMMAND")
        .args(["sh", "-c", "echo \"$GIT_SSH_COMMAND\""]);

    // When the command runs
    let output = cmd.output().unwrap();

    // Then the child was handed an ssh whose ProxyCommand re-executes sandme in
    // tunnel mode — git-over-SSH is routed through the proxy with no
    // configuration by the user (SPEC-0012 FR-1201, NFR-1201)
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("ssh -o ProxyCommand=") && stdout.contains("--sandme-ssh-connect %h %p"),
        "GIT_SSH_COMMAND should point ssh at the tunnel; got: {stdout:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn keeps_a_user_supplied_git_ssh_command() {
    // Given the user set their own GIT_SSH_COMMAND before invoking sandme
    let dir = workdir("keeps-a-user-supplied-git-ssh-command");
    let mut cmd = sandme(&dir);
    cmd.env("GIT_SSH_COMMAND", "ssh -o SetByTheUser=yes").args([
        "sh",
        "-c",
        "echo \"$GIT_SSH_COMMAND\"",
    ]);

    // When the command runs
    let output = cmd.output().unwrap();

    // Then sandme leaves it untouched: an injected variable must not overwrite
    // a value the user set (SPEC-0012 FR-1202, posix.md §6)
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim_end(),
        "ssh -o SetByTheUser=yes"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn reaches_an_origin_through_the_ssh_connect_tunnel() {
    // Given a raw TCP origin on loopback — standing in for the byte stream an
    // SSH server presents — and sandme with private egress allowed so the proxy
    // will relay to loopback (SPEC-0003 FR-205), keeping this test's subject the
    // tunnel rather than the destination policy
    let origin = tokio::net::TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0)))
        .await
        .unwrap();
    let origin_port = origin.local_addr().unwrap().port();
    tokio::spawn(echo_line_once(origin));

    let dir = workdir("reaches-an-origin-through-the-ssh-tunnel");
    let exe = env!("CARGO_BIN_EXE_sandme");
    let mut cmd = sandme(&dir);
    // The sandboxed shell runs sandme's own ProxyCommand mode, exactly as ssh
    // would: it reads the proxy address and this run's credential from
    // HTTP_PROXY, opens an HTTP CONNECT tunnel to the origin, and relays
    // stdin/stdout across it (SPEC-0012 FR-1203, FR-1204). The helper is
    // exec'd from outside the shared path, which the profile's unscoped
    // `process-exec` already allows — so the tunnel needs no profile change.
    cmd.env("SANDME_ALLOW_PRIVATE_EGRESS", "1").arg(format!(
        "printf 'ping\\n' | '{exe}' --sandme-ssh-connect 127.0.0.1 {origin_port}"
    ));

    // When the sandboxed command runs
    let output = cmd.output().unwrap();

    // Then the origin's reply came back across the tunnel: the credential
    // authorised the CONNECT and bytes crossed both ways
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("tunnelled-pong"),
        "the ssh tunnel did not relay the origin's reply; stdout: {stdout:?}, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// Answer one raw TCP client with a fixed marker, so a test can prove bytes
/// crossed the SSH tunnel in both directions.
async fn echo_line_once(listener: tokio::net::TcpListener) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let Ok((mut stream, _)) = listener.accept().await else {
        return;
    };
    let mut buf = [0_u8; 64];
    let _ = stream.read(&mut buf).await;
    let _ = stream.write_all(b"tunnelled-pong\n").await;
    let _ = stream.shutdown().await;
}
