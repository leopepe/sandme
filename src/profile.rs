//! Generation of the Seatbelt (SBPL) profile applied to the sandboxed command.
//!
//! This module decides what the sandbox permits. Launching a command under a
//! profile is [`crate::sandbox`].

use std::fmt::Write;
use std::net::SocketAddr;

use crate::config::Config;

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
pub fn generate_profile(config: &Config, proxy: SocketAddr) -> String {
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

    append_unconditional_denials(&mut sbpl);
    sbpl
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
fn append_writable_grants(sbpl: &mut String, config: &Config) {
    for path in &config.shared_paths {
        let expanded = expand_path(path);
        let _ = writeln!(
            sbpl,
            "(allow file-read* file-write* (subpath \"{expanded}\"))"
        );
    }

    if config.gui_mode {
        // GUI applications keep state, caches and preferences under ~/Library,
        // and scratch space in the temporary directories. A command that is not
        // a GUI application needs none of it, so none of it is granted unless
        // the user asks for GUI mode (issue #12).
        if let Some(home) = canonical_home() {
            let _ = writeln!(
                sbpl,
                "(allow file-read* file-write* (subpath \"{home}/Library\"))"
            );
        }
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

/// Append the denials no configuration may lift.
///
/// Writes the process-information and `kern.procargs` denials, then the
/// home-relative path denials. The path denials are skipped when `$HOME` does
/// not resolve; the others are written first so they are not lost with it.
///
/// Must be called last. SBPL is last-match-wins for paths, so emitted any
/// earlier the default `shared_paths = ["~/"]` grants `~/Library` straight
/// back and the denial does nothing.
fn append_unconditional_denials(sbpl: &mut String) {
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
    // `(allow process-info-pidinfo)` — including one injected through
    // `shared_paths`, which `append_writable_grants` interpolates unescaped
    // (issue #41). Denying the operation at its own specificity ties with such
    // an allow, leaving last-match-wins to decide it, and still yields to the
    // more specific `(target same-sandbox)` grant above.
    //
    // Stated before the early return below, which must not drop them.
    sbpl.push_str("(deny process-info*)\n");
    sbpl.push_str("(deny process-info-pidinfo)\n");
    sbpl.push_str("(deny sysctl-read (sysctl-name-prefix \"kern.procargs\"))\n");

    let Some(home) = canonical_home() else { return };

    for directory in DENIED_HOME_LIBRARY_DIRECTORIES {
        let _ = writeln!(
            sbpl,
            "(deny file-read* file-write* (subpath \"{home}/Library/{directory}\"))"
        );
    }

    for directory in DENIED_HOME_DIRECTORIES {
        let _ = writeln!(sbpl, "(deny file-write* (subpath \"{home}/{directory}\"))");
    }
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

    fn gui_config_with(paths: &[&str]) -> Config {
        Config {
            gui_mode: true,
            ..config_with(paths)
        }
    }

    #[test]
    fn gui_mode_allows_temp_writes() {
        let profile = generate_profile(
            &gui_config_with(&[]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 8787)),
        );
        assert!(profile.contains("/private/tmp"));
        assert!(profile.contains("/private/var/folders"));
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
        assert!(generate_profile(&gui_config_with(&[]), proxy).contains(&grant));

        // Given no GUI mode, nothing grants it implicitly
        assert!(!generate_profile(&config_with(&[]), proxy).contains(&grant));
    }

    // Serial for the same reason as the test above.
    #[test]
    #[serial_test::serial]
    fn denies_the_launchd_and_keychain_directories_after_every_grant() {
        // Given the home directory shared read-write, as it is by default
        let profile = generate_profile(
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
        let profile = generate_profile(
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
        let profile = generate_profile(
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
