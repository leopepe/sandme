//! Execution of a command under the Seatbelt sandbox.
//!
//! The profile this applies is built by [`crate::profile`].

use std::process::ExitStatus;

use crate::app_bundle;
use crate::error::SandmeError;
use crate::executable;
use crate::proxy;

/// `sandbox-exec`'s exit status when it could not execute the command.
///
/// It answers this one value — `EX_OSERR` from `sysexits.h` — for every
/// `execvp` failure, naming the cause only in a message on the child's
/// stderr, so the status alone tells the caller nothing the exec-wrapper
/// convention recognises. sandme translates it where it can establish the
/// cause itself (SPEC-0004).
const EXEC_FAILED: i32 = 71;

/// The shell the single-operand form is routed through.
///
/// `/bin/bash`, not `/bin/sh`: macOS's `/bin/sh` is bash in POSIX mode, where
/// process substitution (`cat <(echo hi)`) is a syntax error — a common
/// diff/compare pattern for an IDE user, and one that reads like the user's
/// command is wrong rather than like sandme chose the shell (issue #36).
/// `/bin/bash` gives fixed, machine-independent semantics with process
/// substitution enabled; `$SHELL` was rejected because it varies per machine
/// and fish/nushell `-c` semantics differ from POSIX (SPEC-0013).
const SHELL: &str = "/bin/bash";

/// The shell command string handed to `/bin/bash -c` (see [`SHELL`]), with an
/// app-bundle CLI wrapper at its head redirected to the bundle's own executable.
///
/// Only the first word is touched, and only when nothing in it can make it
/// anything other than the command word: a shell metacharacter there would
/// mean an assignment (`FOO=1 zed .`), a redirection (`>log zed .`) or a
/// compound command (`(zed .)`), and rewriting the wrong token is worse than
/// not rewriting at all. What survives that filter is a literal word, which
/// POSIX `sh` can only read as the command word — and it is rewritten only
/// when it names a real app bundle, so a shell keyword (`if`, `time`) falls
/// through untouched. Everything after the first word keeps its shell
/// meaning, so `zed ~/Workspace/` still gets its tilde expanded.
///
/// A wrapper further inside the string — `cd /x && zed .` — is left to the
/// shell: finding it would mean parsing the string, and a wrong guess would
/// silently run something the user never asked for (issue #13).
fn redirect_shell_command(command: &str) -> String {
    let trimmed = command.trim_start();
    let (head, arguments) = trimmed
        .split_once(char::is_whitespace)
        .unwrap_or((trimmed, ""));

    if !is_literal_word(head) {
        return command.to_string();
    }
    let Some(main) = app_bundle::main_executable(head) else {
        return command.to_string();
    };
    format!("{} {arguments}", single_quoted(&main))
}

/// Whether `word` carries no shell meaning beyond naming something.
fn is_literal_word(word: &str) -> bool {
    const SHELL_SPECIAL: [char; 22] = [
        '|', '&', ';', '<', '>', '(', ')', '$', '`', '\\', '"', '\'', '*', '?', '[', ']', '{', '}',
        '~', '!', '=', '#',
    ];

    !word.is_empty() && !word.contains(SHELL_SPECIAL)
}

/// Quote `text` so the shell reads it as one literal word.
fn single_quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// Execute a command under the Seatbelt sandbox and wait for it.
///
/// When the user passes multiple operands (`sandme ls -ltra ./`), each
/// reaches the child unshelled and unquoted: the first is the program,
/// the rest are its arguments (posix.md §5).
///
/// When the user passes a single operand (`sandme 'ls -ltra ./'`), it is
/// treated as a shell command string and routed through `/bin/bash -c` (see
/// [`SHELL`]). This enables shell features — pipes, redirections, globbing,
/// variable expansion, process substitution — inside the quoted command. This
/// matches the convention of `ssh`, `docker exec`, and `tmux new-session`.
///
/// Either way the program is put through [`app_bundle::main_executable`]
/// first, so an IDE named by its CLI wrapper starts inside the sandbox
/// instead of asking a blocked `LaunchServices` to open it.
///
/// The profile is handed to `sandbox-exec -p`, so nothing sensitive
/// touches a temp file. The command's egress is wired to the proxy
/// without any configuration of its own (FR-006): `HTTP_PROXY`/
/// `HTTPS_PROXY` carry the proxy's URL, credential and all (SPEC-0003
/// FR-203), and the profile allows no other network destination. Ctrl-C
/// kills the child so sandme can shut down with it (T-007).
pub async fn run(
    profile: &str,
    proxy: &proxy::Server,
    command: &[String],
) -> Result<ExitStatus, SandmeError> {
    // clap's required trailing operand guarantees at least one word.
    assert!(!command.is_empty(), "clap requires at least one operand");

    // Single-argument: route through the shell so shell features work.
    // Multi-argument: direct exec, first word is the program.
    let (program, args): (String, Vec<String>) = if command.len() == 1 {
        (
            SHELL.to_string(),
            vec!["-c".to_string(), redirect_shell_command(&command[0])],
        )
    } else {
        let (wrapper, rest) = command.split_first().unwrap();
        let program = app_bundle::main_executable(wrapper).unwrap_or_else(|| wrapper.clone());
        (program, rest.to_vec())
    };

    let proxy_url = proxy.url();

    let mut command = tokio::process::Command::new("sandbox-exec");
    command
        .arg("-p")
        .arg(profile)
        .arg(&program)
        .args(&args)
        .env("HTTP_PROXY", &proxy_url)
        .env("HTTPS_PROXY", &proxy_url)
        .env("http_proxy", &proxy_url)
        .env("https_proxy", &proxy_url);
    wire_git_ssh(&mut command);

    let mut child = command.spawn().map_err(SandmeError::Execute)?;

    let status = tokio::select! {
        status = child.wait() => status.map_err(SandmeError::Execute)?,
        _ = tokio::signal::ctrl_c() => {
            let _ = child.kill().await;
            child.wait().await.map_err(SandmeError::Execute)?
        }
    };

    // `sandbox-exec` reached execvp and it failed. Which of the two failures
    // posix.md §4 names it was is not in the status, so it is worked out from
    // the filesystem; a 71 that resolution cannot explain — a profile that
    // would not compile, or a command that ran and chose 71 for itself —
    // passes through untouched (FR-203, FR-204).
    if status.code() == Some(EXEC_FAILED)
        && let Some(error) = executable::exec_failure(&program)
    {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_a_shell_command_alone_when_its_head_is_not_a_bundle() {
        // A name nothing on PATH answers to: there is no bundle to redirect to,
        // so the string reaches the shell exactly as the user typed it.
        let command = "sandme-no-such-command --flag ~/Workspace/";
        assert_eq!(redirect_shell_command(command), command);
    }

    #[test]
    fn leaves_a_shell_command_alone_when_its_head_is_not_the_program() {
        // Each head here is an assignment, a redirection, a compound command or
        // a quoted word — never something sandme may rewrite.
        for command in [
            "FOO=1 zed .",
            ">log zed .",
            "(zed .)",
            "'zed' .",
            "$EDITOR .",
            "",
        ] {
            assert_eq!(redirect_shell_command(command), command);
        }
    }

    #[test]
    fn quotes_a_redirected_program_for_the_shell() {
        assert_eq!(
            single_quoted("/Applications/Visual Studio Code.app/Contents/MacOS/Electron"),
            "'/Applications/Visual Studio Code.app/Contents/MacOS/Electron'"
        );
        assert_eq!(single_quoted("/tmp/it's here"), r"'/tmp/it'\''s here'");
    }
}
