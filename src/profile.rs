//! Generation of the Seatbelt (SBPL) profile applied to the sandboxed command.
//!
//! This module decides what the sandbox permits. Launching a command under a
//! profile is [`crate::sandbox`].
//!
//! This file exceeds the 400-line limit in `docs/guidelines/code/simplicity.md`
//! §2 and takes that guideline's escape hatch (SPEC-0007, SPEC-0009). The overage is
//! tests: the module proper is one cohesive purpose — build the profile — and
//! its unit tests sit at its foot, where `docs/guidelines/code/consistency.md`
//! §4 requires them. Splitting either out is the worse alternative the guideline
//! names.

use std::fmt::Write;
use std::net::SocketAddr;

use crate::config::Config;
use crate::error::SandmeError;

/// Build the Seatbelt (SBPL) profile for one invocation.
///
/// `config` supplies the paths to share and whether GUI mode is on; `proxy` is
/// the address the egress proxy listens on, and the only destination the
/// profile permits.
///
/// Returns the complete profile text: everything denied by default, then the
/// process and file-read grants a command needs to start, the configuration's
/// read-write shares, egress to `proxy`, and last the denials no configuration
/// can lift. Process information is limited to the command's own sandbox
/// instance, which is what keeps another process's environment out of reach
/// (issue #31).
///
/// Nothing may be appended to the result. SBPL resolves a path against the
/// last matching rule, so a rule added after the closing denials would grant
/// their paths back.
///
/// `(deny appleevent-send)` closes cross-application `AppleEvents` (SPEC-0007
/// FR-703): the sandbox has no reason to script other applications.
///
/// The `/dev/ptmx` and `/dev/ttysNNN` rules let the command allocate a
/// pseudo-terminal — a new kernel object it creates, so a real widening
/// (SPEC-0009). All three are required and none suffices alone (bisected in
/// issue #29): `/dev/ptmx` needs read-write, unlocking the slave needs
/// `file-ioctl` on it, and the slave is a `/dev/ttysNNN` device on macOS.
///
/// # Errors
///
/// Returns [`SandmeError::UnsafeProfilePath`] when a configured path — a
/// `shared_paths` entry, `$HOME`, `$TMPDIR` or the working directory — carries
/// a character that could break out of its SBPL string literal. See
/// [`checked_profile_path`].
pub fn generate_profile(config: &Config, proxy: SocketAddr) -> Result<String, SandmeError> {
    let mut sbpl = String::from(
        "(version 1)\n\
         (deny default)\n\
         (allow process-exec)\n\
         (allow process-fork)\n\
         (allow process-info-pidinfo (target same-sandbox))\n\
         (allow signal (target self))\n\
         (allow sysctl-read)\n\
         (allow mach-lookup)\n\
         (allow mach-register)\n\
         (allow mach-bootstrap)\n\
         (allow iokit-open)\n\
         (allow lsopen)\n\
         (allow ipc-posix-shm*)\n\
         (deny appleevent-send)\n\
         (allow file-read-metadata)\n\
         (allow file-read* file-write* (literal \"/dev/ptmx\"))\n\
         (allow file-ioctl (literal \"/dev/ptmx\"))\n\
         (allow file-read* file-write* (regex #\"^/dev/ttys[0-9]+$\"))\n\
         (allow file-read* file-write* (literal \"/dev/tty\") (literal \"/dev/null\"))\n\
         (allow file-write* (literal \"/dev/null\"))\n\
         (allow file-read* file-write* (subpath \"/dev/fd\"))\n\
         (allow file-read* (literal \"/dev/random\") (literal \"/dev/urandom\"))\n\
         (allow file-read* (literal \"/\"))\n\
         (allow file-read* (subpath \"/usr\") (subpath \"/bin\") (subpath \"/sbin\") (subpath \"/System\") (subpath \"/Library\") (subpath \"/Applications\"))\n\
         (allow file-read* (subpath \"/private/etc\") (subpath \"/private/var/db/dyld\") (subpath \"/private/var/run\"))\n",
    );

    append_writable_grants(&mut sbpl, config)?;

    let _ = writeln!(
        sbpl,
        "(allow network-outbound (remote ip \"localhost:{}\"))",
        proxy.port()
    );

    append_unconditional_denials(&mut sbpl)?;
    Ok(sbpl)
}

/// Return `path` if it can go inside an SBPL `"…"` literal, or an error.
///
/// Every configured path — `shared_paths`, `$HOME`, `$TMPDIR`, the working
/// directory — is written between the quotes of a `(subpath "…")` rule from the
/// environment or a config file. A `"` closes the literal and turns what
/// follows into profile syntax, so an injected `(allow default)` is a full
/// sandbox escape (issue #41); a backslash subverts the next quote's escaping.
/// Measured against `sandbox-exec`, those two plus ASCII control characters
/// break out, while parentheses and spaces are inert — so the guard rejects
/// only the first three, fail-shut, and admits ordinary paths.
fn checked_profile_path(path: String) -> Result<String, SandmeError> {
    if path
        .chars()
        .any(|c| c == '"' || c == '\\' || c.is_ascii_control())
    {
        return Err(SandmeError::UnsafeProfilePath { path });
    }
    Ok(path)
}

/// Directories under `~/Library` that no configuration may reach.
///
/// `LaunchAgents` and `LaunchDaemons` are launchd's job directories: a plist
/// written to either runs at the next login, outside the sandbox, so a
/// writable share of one is a persistence escape. `Keychains` holds the login
/// keychain (issue #12).
const DENIED_HOME_LIBRARY_DIRECTORIES: [&str; 3] = ["Keychains", "LaunchAgents", "LaunchDaemons"];

/// Directories under `$HOME` denied write access, whatever the configuration
/// says.
///
/// `.sandme` holds the configuration that decides what the sandbox permits.
/// Writing it cannot widen the current run, whose profile is already loaded,
/// but it sets the terms of the next one.
const DENIED_HOME_DIRECTORIES: [&str; 1] = [".sandme"];

/// Append the read-write grants the configuration asks for.
///
/// Writes one rule per entry in `config.shared_paths`, with `~` expanded and
/// symlinks resolved, and — when `config.gui_mode` is set — the state and
/// scratch directories GUI applications need. These are the only rules in the
/// profile that vary per invocation.
fn append_writable_grants(sbpl: &mut String, config: &Config) -> Result<(), SandmeError> {
    for path in &config.shared_paths {
        let expanded = checked_profile_path(expand_path(path))?;
        let _ = writeln!(
            sbpl,
            "(allow file-read* file-write* (subpath \"{expanded}\"))"
        );
    }

    if config.gui_mode {
        // GUI applications keep state, caches and preferences under ~/Library,
        // and scratch space in the per-user temporary directory. A command that
        // is not a GUI application needs none of it, so none of it is granted
        // unless the user asks for GUI mode (issue #12).
        if let Some(home) = canonical_home() {
            let home = checked_profile_path(home)?;
            let _ = writeln!(
                sbpl,
                "(allow file-read* file-write* (subpath \"{home}/Library\"))"
            );
        }
        // Scope the temp grant to the invoking user's own $TMPDIR, not all of
        // /private/tmp (world-shared) or /private/var/folders (every account's
        // per-app containers) — issue #12 §3. $TMPDIR is what the child itself
        // uses for temp files, so this grants precisely where it writes.
        if let Some(temp) = user_temp_dir() {
            let temp = checked_profile_path(temp)?;
            let _ = writeln!(sbpl, "(allow file-read* file-write* (subpath \"{temp}\"))");
        }
    }

    Ok(())
}

/// The per-user temporary directory as the kernel sees it, or `None` if
/// `$TMPDIR` is unset.
///
/// `$TMPDIR` names `_CS_DARWIN_USER_TEMP_DIR`, the scratch directory macOS gives
/// each login session — the one an editor, macOS `diff` on a process
/// substitution, or git's `xcrun` shim (issue #29) writes into. Symlinks are
/// resolved so the rule matches the kernel's view of the path, as with
/// `shared_paths`: `$TMPDIR` reaches its target through `/var`, which resolves
/// to `/private/var`, and a grant naming the unresolved form would not match.
fn user_temp_dir() -> Option<String> {
    let tmpdir = std::env::var("TMPDIR").ok()?;
    Some(std::fs::canonicalize(&tmpdir).map_or(tmpdir, |resolved| resolved.display().to_string()))
}

/// Append the denials no configuration may lift.
///
/// Writes the process-information and `kern.procargs` denials, then the
/// home-relative path denials. The path denials are skipped when `$HOME` does
/// not resolve; the others are written first so they are not lost with it.
///
/// Must be called last. SBPL is last-match-wins for paths, so emitted any
/// earlier the default share would grant `~/Library` straight back and the
/// denial does nothing.
fn append_unconditional_denials(sbpl: &mut String) -> Result<(), SandmeError> {
    // `kern.procargs2` returns any same-uid process's environment, so reading
    // it hands the command every credential the user gave any other program
    // (issue #31). All three rules are needed and each looks redundant beside
    // the others: `(deny default)` does not reach this read, the wildcard does
    // not survive a specific allow, and the named denial does not cover the
    // rest of the family. Measured on macOS 26 — drop any one and the read
    // succeeds.
    //
    // SBPL prefers a *specific* allow to a *wildcard* deny whatever the rule
    // order, so `(deny process-info*)` alone is defeated by any unscoped
    // `(allow process-info-pidinfo)`. Denying the operation at its own
    // specificity ties with such an allow, leaving last-match-wins to decide
    // it, and still yields to the more specific `(target same-sandbox)` grant
    // above — defence-in-depth now that `checked_profile_path` rejects the
    // `shared_paths`/`$TMPDIR` injection that could plant one (issue #41).
    //
    // Stated before the early return below, which must not drop them.
    sbpl.push_str("(deny process-info*)\n");
    sbpl.push_str("(deny process-info-pidinfo)\n");
    sbpl.push_str("(deny sysctl-read (sysctl-name-prefix \"kern.procargs\"))\n");

    let Some(home) = canonical_home() else {
        return Ok(());
    };
    let home = checked_profile_path(home)?;

    for directory in DENIED_HOME_LIBRARY_DIRECTORIES {
        let _ = writeln!(
            sbpl,
            "(deny file-read* file-write* (subpath \"{home}/Library/{directory}\"))"
        );
    }

    for directory in DENIED_HOME_DIRECTORIES {
        let _ = writeln!(sbpl, "(deny file-write* (subpath \"{home}/{directory}\"))");
    }

    Ok(())
}

/// The home directory as the kernel sees it.
///
/// Returns `$HOME` with symlinks resolved, the raw value if it cannot be
/// resolved, or `None` if `$HOME` is unset. Rules must be written from the
/// resolved form: `/var/…` and `/private/var/…` are the same directory but not
/// the same subpath, and a denial that does not match does not deny.
fn canonical_home() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    Some(std::fs::canonicalize(&home).map_or(home, |resolved| resolved.display().to_string()))
}

/// Expand a leading `~/` and resolve symlinks, so a rule built from `path`
/// matches the kernel's view of it.
///
/// Returns `path` unchanged when `$HOME` is unset or the path does not exist.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};

    /// The profile for a valid config. The tests here build from paths with no
    /// SBPL-breaking characters, so generation never returns the error
    /// [`checked_profile_path`] guards; the injection cases assert on
    /// [`generate_profile`]'s `Result` directly.
    fn profile_of(config: &Config, proxy: SocketAddr) -> String {
        generate_profile(config, proxy).expect("a valid config generates a profile")
    }

    fn config_with(paths: &[&str]) -> Config {
        Config {
            shared_paths: paths.iter().map(|p| (*p).to_string()).collect(),
            proxy_port: 8787,
            gui_mode: false,
            allow_private_egress: false,
        }
    }

    #[test]
    fn denies_everything_by_default() {
        let profile = profile_of(
            &config_with(&[]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        assert!(profile.starts_with("(version 1)\n(deny default)\n"));
    }

    #[test]
    fn shares_configured_paths_read_write() {
        let profile = profile_of(
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
        let profile = profile_of(
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
        let profile = profile_of(&config_with(&[]), proxy);

        assert!(profile.contains("(allow network-outbound (remote ip \"localhost:8787\"))"));
        assert!(!profile.contains("network-bind"));
    }

    fn gui_config_with(paths: &[&str]) -> Config {
        Config {
            gui_mode: true,
            ..config_with(paths)
        }
    }

    #[test]
    fn gui_mode_grants_the_per_user_temp_dir_only() {
        let profile = profile_of(
            &gui_config_with(&[]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 8787)),
        );
        let temp = user_temp_dir().expect("cargo test runs with TMPDIR set");

        // Given GUI mode, the invoking user's own temp dir is granted
        assert!(profile.contains(&format!(
            "(allow file-read* file-write* (subpath \"{temp}\"))"
        )));

        // And the broad temp surfaces are not: not world-shared /private/tmp,
        // nor every account's containers under /private/var/folders (issue #12 §3)
        assert!(!profile.contains("(subpath \"/private/tmp\")"));
        assert!(!profile.contains("(subpath \"/private/var/folders\")"));
    }

    #[test]
    fn denies_the_appleevent_send_operation() {
        // Given any profile, GUI mode or not
        let profile = profile_of(
            &config_with(&[]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 8787)),
        );

        // Then cross-application AppleEvents are denied (SPEC-0007 FR-703)
        assert!(profile.contains("(deny appleevent-send)"));
    }

    // `$HOME` is process-wide, and config.rs's tests reassign it while they
    // run. This test and the next read it twice — once here, once inside the
    // profile — and need both reads to see the same value, so they queue
    // behind those (serial_test's default key is shared crate-wide).
    #[test]
    #[serial_test::serial]
    fn grants_the_home_library_only_in_gui_mode() {
        let proxy = SocketAddr::from((Ipv4Addr::LOCALHOST, 1));
        let home = canonical_home().expect("cargo test runs with HOME set");
        let grant = format!("(allow file-read* file-write* (subpath \"{home}/Library\"))");

        // Given GUI mode, the state directory GUI applications need is granted
        assert!(profile_of(&gui_config_with(&[]), proxy).contains(&grant));

        // Given no GUI mode, nothing grants it implicitly
        assert!(!profile_of(&config_with(&[]), proxy).contains(&grant));
    }

    // Serial for the same reason as the test above.
    #[test]
    #[serial_test::serial]
    fn denies_the_launchd_and_keychain_directories_after_every_grant() {
        // Given the home directory shared read-write, as it is by default
        let profile = profile_of(
            &gui_config_with(&["~/"]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );
        let home = canonical_home().expect("cargo test runs with HOME set");

        // Then each denial is present, and every one of them follows the last
        // allow rule in the profile: SBPL is last-match-wins, so a denial that
        // preceded a grant of `~/` would be overridden by it.
        let last_allow = profile.rfind("(allow ").expect("the profile grants");
        for directory in DENIED_HOME_LIBRARY_DIRECTORIES {
            let denial =
                format!("(deny file-read* file-write* (subpath \"{home}/Library/{directory}\"))");
            let at = profile
                .find(&denial)
                .unwrap_or_else(|| panic!("profile is missing: {denial}"));
            assert!(at > last_allow, "{denial} must come after every allow rule");
        }
    }

    #[test]
    fn denies_process_information_after_every_grant() {
        // Given the widest configuration: GUI mode and the home directory shared
        let profile = profile_of(
            &gui_config_with(&["~/"]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        // Then all three denials are present and follow the last allow rule.
        // None closes issue #31 alone, so an edit dropping one has to fail
        // here rather than quietly reopen the leak.
        let last_allow = profile.rfind("(allow ").expect("the profile grants");
        for denial in [
            "(deny process-info*)",
            "(deny process-info-pidinfo)",
            "(deny sysctl-read (sysctl-name-prefix \"kern.procargs\"))",
        ] {
            let at = profile
                .find(denial)
                .unwrap_or_else(|| panic!("profile is missing: {denial}"));
            assert!(at > last_allow, "{denial} must come after every allow rule");
        }

        // And no *unscoped* process-info grant survives. SBPL prefers a
        // specific allow to a wildcard deny whatever the order, so a bare
        // `(allow process-info-pidinfo)` reopens the leak while every other
        // assertion here still passes.
        assert!(
            !profile.contains("(allow process-info-pidinfo)"),
            "an unscoped process-info grant outranks (deny process-info*) and reopens issue #31"
        );

        // And the sysctls ordinary programs read are untouched: the denial
        // filters one prefix, where denying the operation takes `hw.ncpu` with
        // it.
        assert!(profile.contains("(allow sysctl-read)"));
    }

    // Serial because it reassigns `$HOME`, which is process-wide.
    #[test]
    #[serial_test::serial]
    fn denies_process_information_without_a_home() {
        // Given no `$HOME` for the home-relative denials to be built from
        let original = std::env::var("HOME").ok();
        unsafe { std::env::remove_var("HOME") };
        let profile = profile_of(
            &config_with(&[]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );
        if let Some(home) = original {
            unsafe { std::env::set_var("HOME", home) };
        }

        // Then the `$HOME`-independent denials survive its absence.
        assert!(profile.contains("(deny process-info*)"));
        assert!(profile.contains("(deny sysctl-read (sysctl-name-prefix \"kern.procargs\"))"));
    }

    #[test]
    fn grants_dev_fd_read_write() {
        // Given the base profile, with no shared paths
        let profile = profile_of(
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
        let profile = profile_of(
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

    #[test]
    fn grants_pseudo_terminal_allocation() {
        // Given the base profile, with no shared paths
        let profile = profile_of(
            &config_with(&[]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        // Then both rules PTY allocation needs are present: the ioctl on the
        // multiplexer that grants the slave, and read-write to the macOS slave
        // devices (issue #29). Neither works without the other.
        assert!(profile.contains("(allow file-ioctl (literal \"/dev/ptmx\"))"));
        assert!(profile.contains("(allow file-read* file-write* (regex #\"^/dev/ttys[0-9]+$\"))"));

        // And `file-ioctl` reaches the multiplexer only, never a slave device
        // (NFR-902). The slave read-write grant matches every same-uid ttys, so
        // withholding the slave ioctl is what keeps `TIOCSTI` line-injection
        // and `TIOCSCTTY` off a foreign terminal — the load-bearing line
        // between this grant and an escape.
        assert_eq!(
            profile.matches("(allow file-ioctl").count(),
            1,
            "file-ioctl must be granted on /dev/ptmx alone"
        );
        assert!(!profile.contains("(allow file-ioctl (regex"));
    }

    #[test]
    fn refuses_a_shared_path_that_would_break_out_of_the_profile() {
        // Given a shared path carrying a quote that closes the subpath literal
        // and appends an unrestricted grant
        let injection = "/tmp/x\")) (allow default) (allow file-read* (subpath \"/";
        let result = generate_profile(
            &config_with(&[injection]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        // Then generation fails shut rather than emit the injected profile
        assert!(matches!(result, Err(SandmeError::UnsafeProfilePath { .. })));
    }

    #[test]
    fn admits_a_shared_path_with_parentheses_and_spaces() {
        // Given a path with characters that are inert inside an SBPL literal —
        // the shape of a real macOS project directory
        let profile = profile_of(
            &config_with(&["/tmp/My Project (2024)"]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        // Then it is granted verbatim: the guard rejects only what breaks out
        assert!(
            profile.contains("(allow file-read* file-write* (subpath \"/tmp/My Project (2024)\"))")
        );
    }
}
