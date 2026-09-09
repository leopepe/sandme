//! Generation of the Seatbelt (SBPL) profile applied to the sandboxed command.
//!
//! What the sandbox permits lives here; how the command is started lives in
//! [`crate::sandbox`]. The split follows the one question each answers: this
//! module decides policy, that one applies it.

use std::fmt::Write;
use std::net::SocketAddr;

use crate::config::Config;

/// Generate the Seatbelt (SBPL) profile applied to the sandboxed command.
///
/// Everything is denied by default (FR-004): the command keeps the process
/// mechanics macOS needs to start, read-only access to the system runtime,
/// read-write access to the shared paths, and network egress to the proxy
/// and nothing else (FR-006).
///
/// The profile closes with the denials in [`DENIED_HOME_LIBRARY_DIRECTORIES`],
/// which sit last because SBPL resolves a path against the *last* rule that
/// matches it. Anything appended after them could grant them back.
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

    append_unconditional_denials(&mut sbpl);
    sbpl
}

/// Directories under `~/Library` that no configuration may reach.
///
/// `LaunchAgents` and `LaunchDaemons` are launchd's job directories: a plist
/// written to either one is executed at the next login *outside* the sandbox,
/// which turns any writable share into a persistence escape (issue #12).
/// `Keychains` holds the login keychain. An editor or agent has no business in
/// any of the three, so they are denied rather than left to configuration.
const DENIED_HOME_LIBRARY_DIRECTORIES: [&str; 3] = ["Keychains", "LaunchAgents", "LaunchDaemons"];

/// Directories under `$HOME` denied outright, whatever the configuration says.
///
/// `.sandme` holds the configuration that decides what the sandbox permits. A
/// command able to write it cannot widen the run it is in — the profile is
/// already loaded — but it sets the terms of the next one: `shared_paths = ["/"]`
/// and `allow_private_egress = true` take effect the moment the user runs
/// `sandme` again. The policy must not be writable by what it constrains.
const DENIED_HOME_DIRECTORIES: [&str; 1] = [".sandme"];

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
/// SBPL is last-match-wins, so these MUST be the profile's final rules. Emitted
/// any earlier, the default `shared_paths = ["~/"]` would grant `~/Library`
/// straight back and the denial would silently do nothing for exactly the
/// configuration most users run.
fn append_unconditional_denials(sbpl: &mut String) {
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

/// The home directory as the kernel sees it: `$HOME` with symlinks resolved.
///
/// A rule written from the raw `$HOME` would not match a canonicalised
/// `shared_paths` entry naming the same directory — `/var/…` and
/// `/private/var/…` are the same place but not the same subpath — and a denial
/// that does not match is a denial that does not deny.
fn canonical_home() -> Option<String> {
    let home = std::env::var("HOME").ok()?;
    Some(std::fs::canonicalize(&home).map_or(home, |resolved| resolved.display().to_string()))
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
