//! Seatbelt sandbox profile generation and sandboxed command execution.

use std::fmt::Write;
use std::net::SocketAddr;
use std::process::ExitStatus;

use crate::app_bundle;
use crate::config::Config;
use crate::error::SandmeError;

/// Generate the Seatbelt (SBPL) profile applied to the sandboxed command.
///
/// Everything is denied by default (FR-004): the command keeps the process
/// mechanics macOS needs to start, read-only access to the system runtime,
/// read-write access to the shared paths, and network egress to the proxy
/// and nothing else (FR-006).
///
/// `/dev/fd` sits with the other device rules because shells implement
/// process substitution — `cat <(echo hi)` — by handing the child a
/// `/dev/fd/N` path. Such a path only names a descriptor the process already
/// holds, so allowing it grants no access the process did not already have:
/// it is not a widening of the sandbox the way a path grant is. `/dev/stdin`,
/// `/dev/stdout` and `/dev/stderr` are symlinks into `/dev/fd` and need no
/// rule of their own. `/dev/random` and `/dev/urandom` are the same generator
/// on macOS, are read-only here, and are denied without an explicit rule.
pub fn generate_profile(config: &Config, proxy: SocketAddr) -> String {
    let mut sbpl = String::from(
        "(version 1)\n\
         (deny default)\n\
         (allow process-exec)\n\
         (allow process-fork)\n\
         (allow process-info-pidinfo)\n\
         (allow signal (target self))\n\
         (allow sysctl-read)\n\
         (allow mach-lookup)\n\
         (allow mach-register)\n\
         (allow mach-bootstrap)\n\
         (allow iokit-open)\n\
         (allow lsopen)\n\
         (allow ipc-posix-shm*)\n\
         (allow file-read-metadata)\n\
         (allow file-read* file-write* (literal \"/dev/ptmx\"))\n\
         (allow file-read* file-write* (subpath \"/dev/pts\"))\n\
         (allow file-read* file-write* (literal \"/dev/tty\") (literal \"/dev/null\"))\n\
         (allow file-write* (literal \"/dev/null\"))\n\
         (allow file-read* file-write* (subpath \"/dev/fd\"))\n\
         (allow file-read* (literal \"/dev/random\") (literal \"/dev/urandom\"))\n\
         (allow file-read* (literal \"/\"))\n\
         (allow file-read* (subpath \"/usr\") (subpath \"/bin\") (subpath \"/sbin\") (subpath \"/System\") (subpath \"/Library\") (subpath \"/Applications\"))\n\
         (allow file-read* (subpath \"/private/etc\") (subpath \"/private/var/db/dyld\") (subpath \"/private/var/run\"))\n",
    );

    append_writable_grants(&mut sbpl, config);

    let _ = writeln!(
        sbpl,
        "(allow network-outbound (remote ip \"localhost:{}\"))",
        proxy.port()
    );
    sbpl
}

/// Append the read-write grants the configuration asks for (FR-004).
///
/// These are the only rules in the profile that vary per invocation, which is
/// why they live apart from the fixed template above.
fn append_writable_grants(sbpl: &mut String, config: &Config) {
    for path in &config.shared_paths {
        let expanded = expand_path(path);
        let _ = writeln!(
            sbpl,
            "(allow file-read* file-write* (subpath \"{expanded}\"))"
        );
    }

    // GUI applications need write access to ~/Library for state, caches, and preferences
    if let Ok(home) = std::env::var("HOME") {
        let _ = writeln!(
            sbpl,
            "(allow file-read* file-write* (subpath \"{home}/Library\"))"
        );
    }

    // GUI mode: allow write access to temporary directories
    if config.gui_mode {
        let _ = writeln!(
            sbpl,
            "(allow file-read* file-write* (subpath \"/private/tmp\"))"
        );
        let _ = writeln!(
            sbpl,
            "(allow file-read* file-write* (subpath \"/private/var/folders\"))"
        );
    }
}

/// Expand a path starting with ~ to the home directory, and resolve
/// symlinks so the profile matches the kernel's view of the path.
fn expand_path(path: &str) -> String {
    let expanded = if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        format!("{home}/{rest}")
    } else {
        path.to_string()
    };
    std::fs::canonicalize(&expanded).map_or(expanded, |resolved| resolved.display().to_string())
}

/// The shell command string handed to `/bin/sh -c`, with an app-bundle CLI
/// wrapper at its head redirected to the bundle's own executable.
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
/// treated as a shell command string and routed through `/bin/sh -c`.
/// This enables shell features — pipes, redirections, globbing, variable
/// expansion — inside the quoted command. This matches the convention of
/// `ssh`, `docker exec`, and `tmux new-session`.
///
/// Either way the program is put through [`app_bundle::main_executable`]
/// first, so an IDE named by its CLI wrapper starts inside the sandbox
/// instead of asking a blocked `LaunchServices` to open it.
///
/// The profile is handed to `sandbox-exec -p`, so nothing sensitive
/// touches a temp file. The command's egress is wired to the proxy
/// without any configuration of its own (FR-006): `HTTP_PROXY`/
/// `HTTPS_PROXY` point at `proxy`, and the profile allows no other
/// network destination. Ctrl-C kills the child so sandme can shut down
/// with it (T-007).
pub async fn run(
    config: &Config,
    proxy: SocketAddr,
    command: &[String],
) -> Result<ExitStatus, SandmeError> {
    // clap's required trailing operand guarantees at least one word.
    assert!(!command.is_empty(), "clap requires at least one operand");

    // Single-argument: route through /bin/sh -c so shell features work.
    // Multi-argument: direct exec, first word is the program.
    let (program, args): (String, Vec<String>) = if command.len() == 1 {
        (
            "/bin/sh".to_string(),
            vec!["-c".to_string(), redirect_shell_command(&command[0])],
        )
    } else {
        let (wrapper, rest) = command.split_first().unwrap();
        let program = app_bundle::main_executable(wrapper).unwrap_or_else(|| wrapper.clone());
        (program, rest.to_vec())
    };

    let profile = generate_profile(config, proxy);
    let proxy_url = format!("http://{proxy}");

    let mut child = tokio::process::Command::new("sandbox-exec")
        .arg("-p")
        .arg(&profile)
        .arg(&program)
        .args(&args)
        .env("HTTP_PROXY", &proxy_url)
        .env("HTTPS_PROXY", &proxy_url)
        .env("http_proxy", &proxy_url)
        .env("https_proxy", &proxy_url)
        .spawn()
        .map_err(SandmeError::Execute)?;

    let status = tokio::select! {
        status = child.wait() => status.map_err(SandmeError::Execute)?,
        _ = tokio::signal::ctrl_c() => {
            let _ = child.kill().await;
            child.wait().await.map_err(SandmeError::Execute)?
        }
    };

    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};

    fn config_with(paths: &[&str]) -> Config {
        Config {
            shared_paths: paths.iter().map(|p| (*p).to_string()).collect(),
            proxy_port: 8787,
            gui_mode: false,
        }
    }

    #[test]
    fn denies_everything_by_default() {
        let profile = generate_profile(
            &config_with(&[]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        assert!(profile.starts_with("(version 1)\n(deny default)\n"));
    }

    #[test]
    fn shares_configured_paths_read_write() {
        let profile = generate_profile(
            &config_with(&["/tmp/sandme-test"]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        assert!(profile.contains("(allow file-read* file-write* (subpath \"/tmp/sandme-test\"))"));
    }

    #[test]
    fn uses_macos_seatbelt_framework() {
        // Given a sandboxed command invocation
        // (this test documents that sandme uses sandbox-exec, the macOS
        // Seatbelt interface, as required by NFR-001)
        let profile = generate_profile(
            &config_with(&["/tmp/test"]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 8787)),
        );

        // Then the profile is valid SBPL (Seatbelt Profile Language)
        // The integration tests in tests/cli.rs exercise sandbox-exec transitively
        assert!(profile.starts_with("(version 1)"));
        assert!(profile.contains("(deny default)"));
    }

    #[test]
    fn routes_network_only_to_the_proxy() {
        let proxy = SocketAddr::from((Ipv4Addr::LOCALHOST, 8787));
        let profile = generate_profile(&config_with(&[]), proxy);

        assert!(profile.contains("(allow network-outbound (remote ip \"localhost:8787\"))"));
        assert!(!profile.contains("network-bind"));
    }

    #[test]
    fn gui_mode_allows_temp_writes() {
        let config = Config {
            shared_paths: vec![],
            proxy_port: 8787,
            gui_mode: true,
        };
        let proxy = SocketAddr::from((Ipv4Addr::LOCALHOST, 8787));
        let profile = generate_profile(&config, proxy);
        assert!(profile.contains("/private/tmp"));
        assert!(profile.contains("/private/var/folders"));
    }

    #[test]
    fn leaves_a_shell_command_alone_when_its_head_is_not_a_bundle() {
        // A name nothing on PATH answers to: there is no bundle to redirect to,
        // so the string reaches /bin/sh exactly as the user typed it.
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

    #[test]
    fn grants_dev_fd_read_write() {
        // Given the base profile, with no shared paths
        let profile = generate_profile(
            &config_with(&[]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        // Then /dev/fd is readable and writable, so the `/dev/fd/N` paths a
        // shell hands to process substitution resolve
        assert!(profile.contains("(allow file-read* file-write* (subpath \"/dev/fd\"))"));
    }

    #[test]
    fn grants_the_random_devices_read_only() {
        // Given the base profile, with no shared paths
        let profile = generate_profile(
            &config_with(&[]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        // Then the random devices are readable, and nothing grants a write
        assert!(
            profile.contains(
                "(allow file-read* (literal \"/dev/random\") (literal \"/dev/urandom\"))"
            )
        );
        assert!(!profile.contains("file-write* (literal \"/dev/urandom\")"));
    }
}
