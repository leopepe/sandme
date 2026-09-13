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
use std::path::{Path, PathBuf};

use crate::config::Config;
use crate::error::SandmeError;

/// Build the Seatbelt (SBPL) profile for one invocation.
///
/// `config` supplies the paths to share and whether GUI mode is on; `proxy` is
/// the address the egress proxy listens on when one is running — the only
/// destination the profile permits — or `None` when the proxy is disabled, in
/// which case no egress line is emitted and the command has no network egress
/// at all (design D1/D2).
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
/// pseudo-terminal and drive it as a terminal — a real widening (SPEC-0009).
/// Allocation needs read-write on `/dev/ptmx` and `file-ioctl` on it (to unlock
/// the slave); a terminal emulator additionally claims the slave as its
/// controlling terminal (`TIOCSCTTY`) and sets its size and modes, which is
/// `file-ioctl` on the `/dev/ttysNNN` slave. SBPL cannot filter by ioctl
/// request, so this grant also permits `TIOCSTI` on any same-uid terminal; that
/// residual is accepted as the cost of a working terminal (SPEC-0009/NFR-902,
/// issue #29).
///
/// # Errors
///
/// Returns [`SandmeError::UnsafeProfilePath`] when a configured path — a
/// `shared_paths` entry, `$HOME`, `$TMPDIR` or the working directory — carries
/// a character that could break out of its SBPL string literal. See
/// [`checked_profile_path`].
pub fn generate_profile(config: &Config, proxy: Option<SocketAddr>) -> Result<String, SandmeError> {
    // Resolve `$HOME` once at this adapter boundary and inject it into the pure
    // decision helpers below, mirroring the Landlock `plan_env()` precedent. The
    // lookup stays on `std::env::var` (String; `Err`/absent on a non-UTF-8
    // value), never its lossy `OsString` cousin: a non-UTF-8 `$HOME` must map to
    // the absent case so the SBPL stays byte-for-byte identical for a given home
    // (design D2).
    let home = std::env::var("HOME").ok().map(PathBuf::from);
    generate_profile_with_home(config, proxy, home.as_deref())
}

/// Build the profile from an explicitly injected home, the pure core of
/// [`generate_profile`]. Tests drive this directly with a synthetic (or absent)
/// home rather than mutating the process `$HOME`.
fn generate_profile_with_home(
    config: &Config,
    proxy: Option<SocketAddr>,
    home: Option<&Path>,
) -> Result<String, SandmeError> {
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
         (allow network-bind (local unix-socket))\n\
         (allow network-outbound (remote unix-socket))\n\
         (allow file-read-metadata)\n\
         (allow file-read* file-write* (literal \"/dev/ptmx\"))\n\
         (allow file-ioctl (literal \"/dev/ptmx\"))\n\
         (allow file-read* file-write* (regex #\"^/dev/ttys[0-9]+$\"))\n\
         (allow file-ioctl (regex #\"^/dev/ttys[0-9]+$\"))\n\
         (allow file-read* file-write* (literal \"/dev/tty\") (literal \"/dev/null\"))\n\
         (allow file-write* (literal \"/dev/null\"))\n\
         (allow file-read* file-write* (subpath \"/dev/fd\"))\n\
         (allow file-read* (literal \"/dev/random\") (literal \"/dev/urandom\"))\n\
         (allow file-read* (literal \"/\"))\n\
         (allow file-read* (subpath \"/usr\") (subpath \"/bin\") (subpath \"/sbin\") (subpath \"/System\") (subpath \"/Library\") (subpath \"/Applications\"))\n\
         (allow file-read* (subpath \"/private/etc\") (subpath \"/private/var/db/dyld\") (subpath \"/private/var/run\"))\n",
    );

    append_writable_grants(&mut sbpl, config, home)?;

    // The one egress line is emitted only when a proxy is running (design D2).
    // When the proxy is off, the base `(deny default)` already denies all
    // outbound network, so the command has no egress — the default path (Some,
    // empty read_only_paths) stays byte-for-byte identical (NFR-1601).
    if let Some(proxy) = proxy {
        let _ = writeln!(
            sbpl,
            "(allow network-outbound (remote ip \"localhost:{}\"))",
            proxy.port()
        );
    }

    // Read-only grants come after the writable grants and the conditional egress
    // line, but before the unconditional denials, so the closing denials still
    // win last-match and a read-only grant cannot resurrect a denied tree
    // (design D5). Empty `read_only_paths` appends nothing (NFR-1601).
    append_read_only_grants(&mut sbpl, config, home)?;

    append_unconditional_denials(&mut sbpl, home)?;
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
fn append_writable_grants(
    sbpl: &mut String,
    config: &Config,
    home: Option<&Path>,
) -> Result<(), SandmeError> {
    for path in &config.shared_paths {
        let expanded = checked_profile_path(expand_path(path, home))?;
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
        if let Some(home) = canonical_home(home) {
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

/// Append the read-only grants the configuration asks for (design D5).
///
/// Writes one `(allow file-read* (subpath "…"))` rule per entry in
/// `config.read_only_paths` — read (and execute), never `file-write*` — with
/// `~` expanded and symlinks resolved, reusing the same [`expand_path`] and
/// [`checked_profile_path`] guard as the writable grants. Must be called after
/// the writable grants and before [`append_unconditional_denials`], so the
/// closing denials still win last-match. Empty `read_only_paths` writes
/// nothing, keeping the default profile byte-for-byte unchanged (NFR-1601).
fn append_read_only_grants(
    sbpl: &mut String,
    config: &Config,
    home: Option<&Path>,
) -> Result<(), SandmeError> {
    for path in &config.read_only_paths {
        let expanded = checked_profile_path(expand_path(path, home))?;
        let _ = writeln!(sbpl, "(allow file-read* (subpath \"{expanded}\"))");
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
/// home-relative path denials. The path denials are skipped when no home is
/// injected; the others are written first so they are not lost with it.
///
/// Must be called last. SBPL is last-match-wins for paths, so emitted any
/// earlier the default share would grant `~/Library` straight back and the
/// denial does nothing.
fn append_unconditional_denials(sbpl: &mut String, home: Option<&Path>) -> Result<(), SandmeError> {
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

    let Some(home) = canonical_home(home) else {
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

/// The injected home directory as the kernel sees it.
///
/// Returns the injected `home` with symlinks resolved, its raw value if it
/// cannot be resolved, or `None` when no home was injected. Rules must be
/// written from the resolved form: `/var/…` and `/private/var/…` are the same
/// directory but not the same subpath, and a denial that does not match does
/// not deny. The value is injected by [`generate_profile`], never read here
/// from the process environment.
fn canonical_home(home: Option<&Path>) -> Option<String> {
    let home = home?;
    Some(std::fs::canonicalize(home).map_or_else(
        |_| home.display().to_string(),
        |resolved| resolved.display().to_string(),
    ))
}

/// Expand a leading `~/` against the injected `home` and resolve symlinks, so a
/// rule built from `path` matches the kernel's view of it.
///
/// Returns `path` unchanged when no home was injected or the path does not
/// exist. The home is injected by [`generate_profile`], never read here from
/// the process environment.
fn expand_path(path: &str, home: Option<&Path>) -> String {
    let expanded = if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = home
    {
        format!("{}/{rest}", home.display())
    } else {
        path.to_string()
    };
    std::fs::canonicalize(&expanded).map_or(expanded, |resolved| resolved.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};
    use std::path::Path;

    /// A synthetic home the tests inject, so profile generation never reads the
    /// process `$HOME` (and needs no `#[serial]` coupling around it). It does
    /// not exist on disk, so `canonical_home` falls back to this literal — which
    /// keeps the home-relative rules deterministic across machines.
    const TEST_HOME: &str = "/synthetic/home";

    /// The profile for a valid config, built with the synthetic [`TEST_HOME`]
    /// injected. The tests here build from paths with no SBPL-breaking
    /// characters, so generation never returns the error [`checked_profile_path`]
    /// guards; the injection cases assert on the builder's `Result` directly.
    fn profile_of(config: &Config, proxy: SocketAddr) -> String {
        profile_of_with_home(config, proxy, Some(Path::new(TEST_HOME)))
    }

    /// The profile for a valid config and an explicitly injected home (present
    /// or absent), so home-dependent behavior is exercised by argument rather
    /// than by mutating the process environment.
    fn profile_of_with_home(config: &Config, proxy: SocketAddr, home: Option<&Path>) -> String {
        generate_profile_with_home(config, Some(proxy), home)
            .expect("a valid config generates a profile")
    }

    fn config_with(paths: &[&str]) -> Config {
        Config {
            shared_paths: paths.iter().map(|p| (*p).to_string()).collect(),
            read_only_paths: Vec::new(),
            proxy: true,
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

        // Unix-domain socket IPC is permitted both ways — the bind Zed's git
        // askpass makes and the connect its helper makes back (issue #29) — but
        // nothing wider: no TCP/UDP listener, and no network-outbound to an IP
        // beyond the proxy.
        assert!(profile.contains("(allow network-bind (local unix-socket))"));
        assert!(profile.contains("(allow network-outbound (remote unix-socket))"));
        assert!(!profile.contains("network-bind (local ip"));
        assert!(!profile.contains("(allow network-outbound (remote ip \"*"));
        assert!(!profile.contains("network-inbound"));
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

    #[test]
    fn grants_the_home_library_only_in_gui_mode() {
        let proxy = SocketAddr::from((Ipv4Addr::LOCALHOST, 1));
        let home = canonical_home(Some(Path::new(TEST_HOME))).expect("a home is injected");
        let grant = format!("(allow file-read* file-write* (subpath \"{home}/Library\"))");

        // Given GUI mode, the state directory GUI applications need is granted
        assert!(profile_of(&gui_config_with(&[]), proxy).contains(&grant));

        // Given no GUI mode, nothing grants it implicitly
        assert!(!profile_of(&config_with(&[]), proxy).contains(&grant));
    }

    #[test]
    fn denies_the_launchd_and_keychain_directories_after_every_grant() {
        // Given the home directory shared read-write, as it is by default
        let profile = profile_of(
            &gui_config_with(&["~/"]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );
        let home = canonical_home(Some(Path::new(TEST_HOME))).expect("a home is injected");

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

    #[test]
    fn denies_process_information_without_a_home() {
        // Given no home injected for the home-relative denials to be built from
        let profile = profile_of_with_home(
            &config_with(&[]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
            None,
        );

        // Then the home-independent denials survive its absence.
        assert!(profile.contains("(deny process-info*)"));
        assert!(profile.contains("(deny sysctl-read (sysctl-name-prefix \"kern.procargs\"))"));

        // And the home-relative denials are omitted: with no home to build them
        // from, nothing names `~/Library/Keychains` or the `.sandme` marker (the
        // spec "No home injected" scenario).
        assert!(!profile.contains("/Library/Keychains"));
        assert!(!profile.contains(".sandme"));
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
    fn grants_pseudo_terminal_allocation_and_control() {
        // Given the base profile, with no shared paths
        let profile = profile_of(
            &config_with(&[]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        // Then the rules a terminal needs are present: read-write on the
        // multiplexer and the macOS slave devices, and `file-ioctl` on both.
        // `file-ioctl` on `/dev/ptmx` unlocks the slave at allocation; the same
        // on the slave lets a terminal emulator claim it as its controlling
        // terminal (`TIOCSCTTY`) and set its size and modes (issue #29).
        assert!(profile.contains("(allow file-read* file-write* (literal \"/dev/ptmx\"))"));
        assert!(profile.contains("(allow file-ioctl (literal \"/dev/ptmx\"))"));
        assert!(profile.contains("(allow file-read* file-write* (regex #\"^/dev/ttys[0-9]+$\"))"));
        assert!(profile.contains("(allow file-ioctl (regex #\"^/dev/ttys[0-9]+$\"))"));
    }

    #[test]
    fn refuses_a_shared_path_that_would_break_out_of_the_profile() {
        // Given a shared path carrying a quote that closes the subpath literal
        // and appends an unrestricted grant
        let injection = "/tmp/x\")) (allow default) (allow file-read* (subpath \"/";
        let result = generate_profile_with_home(
            &config_with(&[injection]),
            Some(SocketAddr::from((Ipv4Addr::LOCALHOST, 1))),
            Some(Path::new(TEST_HOME)),
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

    /// A config with the given read-only paths and nothing else configured.
    fn read_only_config_with(paths: &[&str]) -> Config {
        Config {
            read_only_paths: paths.iter().map(|p| (*p).to_string()).collect(),
            ..config_with(&[])
        }
    }

    #[test]
    fn omits_the_egress_line_when_the_proxy_is_disabled() {
        // Given the proxy is off (None), the network-outbound egress line is
        // never emitted (design D2) — the base (deny default) denies all egress
        let profile =
            generate_profile_with_home(&config_with(&[]), None, Some(Path::new(TEST_HOME)))
                .expect("a valid config generates a profile");

        assert!(
            !profile.contains("network-outbound (remote ip"),
            "no IP egress line may appear when the proxy is off: {profile}"
        );
        // The unix-socket IPC lines are unrelated to the proxy and stay.
        assert!(profile.contains("(allow network-outbound (remote unix-socket))"));
    }

    #[test]
    fn the_default_proxy_on_profile_is_byte_for_byte_unchanged() {
        // The default invocation — proxy on, empty read_only_paths — must
        // produce exactly the profile it produced before this change: the
        // network-outbound line in its original position and no read-only
        // grants (SPEC-0016/NFR-1601). This spells out the whole expected text
        // so any reordering or stray line fails here.
        let profile = generate_profile_with_home(
            &config_with(&[]),
            Some(SocketAddr::from((Ipv4Addr::LOCALHOST, 8787))),
            Some(Path::new(TEST_HOME)),
        )
        .expect("a valid config generates a profile");

        let home = canonical_home(Some(Path::new(TEST_HOME))).expect("a home is injected");
        let expected = format!(
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
             (allow network-bind (local unix-socket))\n\
             (allow network-outbound (remote unix-socket))\n\
             (allow file-read-metadata)\n\
             (allow file-read* file-write* (literal \"/dev/ptmx\"))\n\
             (allow file-ioctl (literal \"/dev/ptmx\"))\n\
             (allow file-read* file-write* (regex #\"^/dev/ttys[0-9]+$\"))\n\
             (allow file-ioctl (regex #\"^/dev/ttys[0-9]+$\"))\n\
             (allow file-read* file-write* (literal \"/dev/tty\") (literal \"/dev/null\"))\n\
             (allow file-write* (literal \"/dev/null\"))\n\
             (allow file-read* file-write* (subpath \"/dev/fd\"))\n\
             (allow file-read* (literal \"/dev/random\") (literal \"/dev/urandom\"))\n\
             (allow file-read* (literal \"/\"))\n\
             (allow file-read* (subpath \"/usr\") (subpath \"/bin\") (subpath \"/sbin\") (subpath \"/System\") (subpath \"/Library\") (subpath \"/Applications\"))\n\
             (allow file-read* (subpath \"/private/etc\") (subpath \"/private/var/db/dyld\") (subpath \"/private/var/run\"))\n\
             (allow network-outbound (remote ip \"localhost:8787\"))\n\
             (deny process-info*)\n\
             (deny process-info-pidinfo)\n\
             (deny sysctl-read (sysctl-name-prefix \"kern.procargs\"))\n\
             (deny file-read* file-write* (subpath \"{home}/Library/Keychains\"))\n\
             (deny file-read* file-write* (subpath \"{home}/Library/LaunchAgents\"))\n\
             (deny file-read* file-write* (subpath \"{home}/Library/LaunchDaemons\"))\n\
             (deny file-write* (subpath \"{home}/.sandme\"))\n"
        );

        assert_eq!(profile, expected);
    }

    #[test]
    fn grants_a_read_only_path_read_without_write() {
        // Given a configured read-only path (an inert literal, so the guard
        // admits it), it is granted file-read* subpath and never file-write*
        let profile = profile_of(
            &read_only_config_with(&["/tmp/toolchain"]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        assert!(profile.contains("(allow file-read* (subpath \"/tmp/toolchain\"))"));
        assert!(
            !profile.contains("file-write* (subpath \"/tmp/toolchain\")"),
            "a read-only path must not be granted write: {profile}"
        );
    }

    #[test]
    fn read_only_grants_precede_the_unconditional_denials() {
        // Given a read-only path, the closing denials still follow it, so
        // last-match-wins keeps a denied tree denied (design D5)
        let profile = profile_of(
            &read_only_config_with(&["/tmp/toolchain"]),
            SocketAddr::from((Ipv4Addr::LOCALHOST, 1)),
        );

        let grant_at = profile
            .find("(allow file-read* (subpath \"/tmp/toolchain\"))")
            .expect("the read-only grant is present");
        for denial in [
            "(deny process-info*)",
            "(deny sysctl-read (sysctl-name-prefix \"kern.procargs\"))",
        ] {
            let at = profile.find(denial).expect("the denial is present");
            assert!(
                at > grant_at,
                "{denial} must come after the read-only grant"
            );
        }
    }
}
