//! Running a command under the platform's sandbox.
//!
//! [`run`] owns everything that does not depend on the platform — resolving the
//! operands to a program and arguments, wiring the proxy into the child's
//! environment, forwarding Ctrl-C, and translating the exit status. How the
//! restriction itself is built and applied is a [`Backend`]: `seatbelt` on
//! macOS today, a Linux (Landlock) backend later (issue #8, SPEC-0015).

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use crate::config::Config;
use crate::error::SandmeError;
use crate::proxy::{self, Server};

#[cfg(target_os = "macos")]
mod seatbelt;

// Declared unconditionally so its `cfg`-neutral `plan` decision layer compiles
// and unit-tests on every target, including this cycle's macOS host; the
// Landlock backend itself is Linux-only, gated inside the module (SPEC-0015).
mod landlock;

/// The shell the single-operand form is routed through (SPEC-0013).
///
/// `/bin/bash`, not `/bin/sh`: macOS's `/bin/sh` is bash in POSIX mode, where
/// process substitution (`cat <(echo hi)`) is a syntax error (issue #36).
/// `/bin/bash` exists on both macOS and Linux, so the routing itself is shared.
/// Expand a leading `~/` against the injected `home`, leaving anything else
/// unchanged. Shared by both sandbox backends (consistency.md §4); each
/// adapter reads `$HOME` with its own semantics (`env::var` for Seatbelt,
/// `var_os` for Landlock) before calling this.
pub fn expand_tilde(path: &str, home: Option<&Path>) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = home
    {
        return home.join(rest);
    }
    PathBuf::from(path)
}

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
    /// The default uses the SPEC-0013 branch with overridable identity steps.
    fn resolve(&self, command: &[String]) -> (String, Vec<String>) {
        if command.len() == 1 {
            let one = &command[0];
            (
                SHELL.into(),
                vec!["-c".into(), self.redirect_shell_command(one)],
            )
        } else {
            let (head, rest) = command.split_first().expect("run() asserts non-empty");
            (self.redirect_program(head), rest.to_vec())
        }
    }

    /// Override identity step: program word rewrite (e.g. app-bundle redirect).
    fn redirect_program(&self, word: &str) -> String {
        word.to_string()
    }

    /// Override identity step: shell command string rewrite.
    fn redirect_shell_command(&self, command: &str) -> String {
        command.to_string()
    }

    /// Build the child process, restricted per `config` and — when `proxy` is
    /// `Some` — able to reach that address. The returned command is unspawned;
    /// [`run`] wires its environment and drives it.
    ///
    /// `proxy` is `None` when the egress proxy is disabled (`proxy = false`):
    /// the backend then grants no network egress at all. The address is absent
    /// precisely when there is no proxy port to grant, so the off case is
    /// unrepresentable-as-contradictory (SPEC-0017/FR-1701).
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
        proxy: Option<SocketAddr>,
    ) -> Result<tokio::process::Command, SandmeError>;

    /// Translate a finished status into a backend-specific exec failure, or
    /// `None` to pass the status through unchanged.
    fn exec_failure(&self, program: &str, status: ExitStatus) -> Option<SandmeError>;
}

/// Execute a command under the platform sandbox and wait for it.
///
/// Resolves the operands, has the [`Backend`] build the restricted child, and —
/// when a proxy is present — wires it into the child's environment
/// (`HTTP_PROXY`/`HTTPS_PROXY` carry the proxy's URL and credential, SPEC-0003
/// FR-203) and points git's ssh at it, forwards Ctrl-C so sandme shuts down with
/// the child (T-007), and returns the child's status — translated to a
/// not-found/not-executable error where the backend can establish that cause.
///
/// `proxy` is `None` when the egress proxy is disabled (`proxy = false`): no
/// proxy environment is set, git's ssh tunnel is not wired (it tunnels *through*
/// the proxy), and the backend grants no egress — a strictly more restrictive
/// run (SPEC-0017/FR-1701).
///
/// # Errors
///
/// Returns [`SandmeError`] if the restriction cannot be built, the child cannot
/// be spawned, or the backend recognises the status as an exec failure.
pub async fn run(
    config: &Config,
    proxy: Option<&proxy::Server>,
    command: &[String],
) -> Result<ExitStatus, SandmeError> {
    // clap's required trailing operand guarantees at least one word.
    assert!(!command.is_empty(), "clap requires at least one operand");

    #[cfg(target_os = "macos")]
    let backend = seatbelt::Seatbelt;
    #[cfg(target_os = "linux")]
    let backend = landlock::Landlock;
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    compile_error!("sandme supports macOS and Linux only");

    let (program, args) = backend.resolve(command);
    let mut child_command = backend.command(&program, &args, config, proxy.map(Server::addr))?;

    // Only a present proxy wires egress into the child: its URL/credential and
    // the git-over-SSH tunnel both point *at* the proxy, so with none there is
    // nothing to point them at (SPEC-0017/FR-1701). With the proxy off the child
    // gets no HTTP(S)_PROXY and no ssh ProxyCommand — no network remotes at all.
    if let Some(proxy) = proxy {
        let proxy_url = proxy.url();
        child_command
            .env("HTTP_PROXY", &proxy_url)
            .env("HTTPS_PROXY", &proxy_url)
            .env("http_proxy", &proxy_url)
            .env("https_proxy", &proxy_url);
        wire_git_ssh(&mut child_command);
    } else {
        // With the proxy off the child must carry no proxy variable, even one
        // inherited from sandme's own environment: otherwise the sandbox denies
        // the egress but the child still believes a proxy is reachable, and the
        // spec's "none is present in the child env" would hold only on a clean
        // environment. Clear all four spellings so the child env is proxy-free
        // regardless of what sandme was launched with (SPEC-0017/FR-1701).
        child_command
            .env_remove("HTTP_PROXY")
            .env_remove("HTTPS_PROXY")
            .env_remove("http_proxy")
            .env_remove("https_proxy");
    }

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
