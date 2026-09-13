//! The macOS sandbox backend: an SBPL profile applied by `sandbox-exec`.
//!
//! Builds the Seatbelt profile ([`profile`]) from the configuration and hands it
//! to `sandbox-exec -p`, so nothing sensitive touches a temp file. Redirects an
//! app-bundle CLI wrapper to the bundle's own executable ([`app_bundle`]) so an
//! IDE named by its wrapper starts inside the sandbox instead of asking a
//! blocked `LaunchServices` to open it.

use std::net::SocketAddr;
use std::process::ExitStatus;

use super::{Backend, SHELL};
use crate::config::Config;
use crate::error::SandmeError;
use crate::executable;

mod app_bundle;
mod profile;

/// `sandbox-exec`'s exit status when it could not execute the command.
///
/// It answers this one value — `EX_OSERR` from `sysexits.h` — for every
/// `execvp` failure, naming the cause only in a message on the child's stderr,
/// so the status alone tells the caller nothing the exec-wrapper convention
/// recognises. sandme translates it where it can establish the cause itself
/// (SPEC-0004).
const EXEC_FAILED: i32 = 71;

/// The macOS backend.
pub struct Seatbelt;

impl Backend for Seatbelt {
    /// As the shared shell routing, but an app-bundle CLI wrapper at the program
    /// position is redirected to the bundle's own executable.
    fn resolve(&self, command: &[String]) -> (String, Vec<String>) {
        if command.len() == 1 {
            (
                SHELL.to_string(),
                vec!["-c".to_string(), redirect_shell_command(&command[0])],
            )
        } else {
            let (wrapper, rest) = command
                .split_first()
                .expect("run() asserts a non-empty command");
            let program = app_bundle::main_executable(wrapper).unwrap_or_else(|| wrapper.clone());
            (program, rest.to_vec())
        }
    }

    fn command(
        &self,
        program: &str,
        args: &[String],
        config: &Config,
        proxy: SocketAddr,
    ) -> Result<tokio::process::Command, SandmeError> {
        let profile = profile::generate_profile(config, proxy)?;
        let mut command = tokio::process::Command::new("sandbox-exec");
        command.arg("-p").arg(profile).arg(program).args(args);
        Ok(command)
    }

    /// `sandbox-exec` reached execvp and it failed. Which of the two failures
    /// posix.md §4 names it was is not in the status, so it is worked out from
    /// the filesystem; a 71 that resolution cannot explain — a profile that
    /// would not compile, or a command that ran and chose 71 for itself —
    /// passes through untouched (SPEC-0004 FR-803, FR-804).
    fn exec_failure(&self, program: &str, status: ExitStatus) -> Option<SandmeError> {
        if status.code() == Some(EXEC_FAILED) {
            executable::exec_failure(program)
        } else {
            None
        }
    }
}

/// The shell command string handed to `/bin/bash -c` (see [`SHELL`]), with an
/// app-bundle CLI wrapper at its head redirected to the bundle's own executable.
///
/// Only the first word is touched, and only when nothing in it can make it
/// anything other than the command word: a shell metacharacter there would mean
/// an assignment (`FOO=1 zed .`), a redirection (`>log zed .`) or a compound
/// command (`(zed .)`), and rewriting the wrong token is worse than not
/// rewriting at all. What survives that filter is a literal word, which the
/// shell can only read as the command word — and it is rewritten only when it
/// names a real app bundle, so a shell keyword (`if`, `time`) falls through
/// untouched. Everything after the first word keeps its shell meaning, so
/// `zed ~/Workspace/` still gets its tilde expanded.
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
