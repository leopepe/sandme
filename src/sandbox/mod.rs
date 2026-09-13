//! Running a command under the platform's sandbox.
//!
//! [`run`] owns everything that does not depend on the platform — resolving the
//! operands to a program and arguments, wiring the proxy into the child's
//! environment, forwarding Ctrl-C, and translating the exit status. How the
//! restriction itself is built and applied is a [`Backend`]: [`seatbelt`] on
//! macOS today, a Linux (Landlock) backend later (issue #8, SPEC-0015).

use std::net::SocketAddr;
use std::process::ExitStatus;

use crate::config::Config;
use crate::error::SandmeError;
use crate::proxy;

#[cfg(target_os = "macos")]
mod seatbelt;

/// The shell the single-operand form is routed through (SPEC-0013).
///
/// `/bin/bash`, not `/bin/sh`: macOS's `/bin/sh` is bash in POSIX mode, where
/// process substitution (`cat <(echo hi)`) is a syntax error (issue #36).
/// `/bin/bash` exists on both macOS and Linux, so the routing itself is shared.
const SHELL: &str = "/bin/bash";

/// One platform's way of building and applying the sandbox.
///
/// Exactly one implementation is compiled per target — [`run`] selects it with
/// `cfg(target_os)` — so this is a compile-time contract, not runtime
/// polymorphism: there is no `dyn Backend`. A new platform is a new module and
/// one `cfg` arm; the orchestration in [`run`] does not change (SPEC-0015).
pub trait Backend {
    /// Resolve the user's operands to the program to launch and its arguments.
    ///
    /// The default is the cross-platform shell routing ([`shell_route`]). A
    /// backend overrides it to add platform program resolution — the macOS
    /// backend redirects an app-bundle CLI wrapper to the bundle's executable.
    fn resolve(&self, command: &[String]) -> (String, Vec<String>) {
        shell_route(command)
    }

    /// Build the child process, restricted per `config` and able to reach only
    /// `proxy`. The returned command is unspawned; [`run`] wires its environment
    /// and drives it.
    ///
    /// # Errors
    ///
    /// Returns [`SandmeError`] when the restriction cannot be built — for
    /// example a configured path that cannot be placed in the profile.
    fn command(
        &self,
        program: &str,
        args: &[String],
        config: &Config,
        proxy: SocketAddr,
    ) -> Result<tokio::process::Command, SandmeError>;

    /// Translate a finished status into a backend-specific exec failure, or
    /// `None` to pass the status through unchanged.
    fn exec_failure(&self, program: &str, status: ExitStatus) -> Option<SandmeError>;
}

/// Route the operands to a program and arguments, without platform resolution.
///
/// A single operand is a shell command string run through [`SHELL`] `-c`;
/// multiple operands are a direct exec — the first the program, the rest its
/// arguments, passed through unshelled and unquoted (posix.md §5).
fn shell_route(command: &[String]) -> (String, Vec<String>) {
    if command.len() == 1 {
        (
            SHELL.to_string(),
            vec!["-c".to_string(), command[0].clone()],
        )
    } else {
        let (program, args) = command
            .split_first()
            .expect("run() asserts a non-empty command");
        (program.clone(), args.to_vec())
    }
}

/// Execute a command under the platform sandbox and wait for it.
///
/// Resolves the operands, has the [`Backend`] build the restricted child, wires
/// the proxy into its environment (`HTTP_PROXY`/`HTTPS_PROXY` carry the proxy's
/// URL and credential, SPEC-0003 FR-203), forwards Ctrl-C so sandme shuts down
/// with the child (T-007), and returns the child's status — translated to a
/// not-found/not-executable error where the backend can establish that cause.
///
/// # Errors
///
/// Returns [`SandmeError`] if the restriction cannot be built, the child cannot
/// be spawned, or the backend recognises the status as an exec failure.
pub async fn run(
    config: &Config,
    proxy: &proxy::Server,
    command: &[String],
) -> Result<ExitStatus, SandmeError> {
    // clap's required trailing operand guarantees at least one word.
    assert!(!command.is_empty(), "clap requires at least one operand");

    #[cfg(not(target_os = "macos"))]
    compile_error!("sandme supports macOS only; the Linux (Landlock) backend is issue #8");
    #[cfg(target_os = "macos")]
    let backend = seatbelt::Seatbelt;

    let (program, args) = backend.resolve(command);
    let mut child_command = backend.command(&program, &args, config, proxy.addr())?;

    let proxy_url = proxy.url();
    child_command
        .env("HTTP_PROXY", &proxy_url)
        .env("HTTPS_PROXY", &proxy_url)
        .env("http_proxy", &proxy_url)
        .env("https_proxy", &proxy_url);
    wire_git_ssh(&mut child_command);

    let mut child = child_command.spawn().map_err(SandmeError::Execute)?;

    let status = tokio::select! {
        status = child.wait() => status.map_err(SandmeError::Execute)?,
        _ = tokio::signal::ctrl_c() => {
            let _ = child.kill().await;
            child.wait().await.map_err(SandmeError::Execute)?
        }
    };

    if let Some(error) = backend.exec_failure(&program, status) {
        return Err(error);
    }

    Ok(status)
}

/// Point git's `ssh` at sandme's proxy so `git@host:…` reaches its remote
/// (issue #33).
///
/// git-over-SSH cannot use `HTTP_PROXY`, and the profile denies port 22, so an
/// SSH remote fails with an opaque error. `GIT_SSH_COMMAND` gives git an `ssh`
/// whose `ProxyCommand` tunnels through the proxy over HTTP `CONNECT`
/// (SPEC-0012). It is left untouched when the user set their own, so their
/// configuration wins (posix.md §6), and skipped when sandme cannot locate its
/// own executable — the tunnel's `ProxyCommand` re-executes it by absolute
/// path, so without that path there is nothing to point ssh at.
fn wire_git_ssh(command: &mut tokio::process::Command) {
    if std::env::var_os("GIT_SSH_COMMAND").is_some() {
        return;
    }
    if let Ok(exe) = std::env::current_exe() {
        command.env("GIT_SSH_COMMAND", crate::tunnel::git_ssh_command(&exe));
    }
}
