//! The Landlock access decision layer: a pure mapping from configuration to an
//! [`AccessPlan`] of paths, rights and ports.
//!
//! This module is compiled on every target (it is `cfg`-neutral) so the policy
//! decision can be unit-tested on macOS without a Linux kernel. It is the
//! Landlock analogue of `crate::sandbox::seatbelt::profile`, minus the SBPL
//! text and — crucially — minus any live-filesystem work: [`build_plan`] is a
//! pure function of its injected inputs and never calls `canonicalize`/`stat`
//! or reads the process environment. The `~`/symlink resolution of
//! `shared_paths` and the reading of the real `HOME`/`XDG_*` environment live
//! in the Linux-only adapter in [`super`] (its `mod.rs`), which hands this
//! module already-resolved absolute paths and an injected [`PlanEnv`].

use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};

use crate::config::Config;
use crate::error::SandmeError;

/// Top-level system directories granted read **and execute**, so a command and
/// its dynamic linker can start and run.
///
/// `/proc` is deliberately absent (SPEC-0016/FR-1607): Landlock has no deny
/// primitive, so the only way to keep another process's `/proc/<pid>/environ`
/// out of reach is to never grant `/proc`. `/sys` is likewise omitted. Because
/// these are the directories the child's `PATH` reaches, they also make the
/// pre-spawn exec-failure resolution meaningful (SPEC-0016/FR-1610): a program
/// found here is executable under Landlock too.
const READ_EXEC_DIRS: [&str; 6] = ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/opt"];

/// Directories and device files granted read only — configuration, certificates
/// and the random devices a command reads but never executes.
const READ_DIRS: [&str; 4] = ["/etc", "/dev/random", "/dev/urandom", "/dev/zero"];

/// Device files granted read-write, including the pseudo-terminal multiplexer
/// `/dev/ptmx` and the `/dev/pts` slave subtree (SPEC-0016/FR-1609, superseding
/// SPEC-0009's Linux decline). `/dev/null` and `/dev/tty` are the terminal
/// basics a command needs to run.
const DEVICE_READ_WRITE: [&str; 4] = ["/dev/null", "/dev/tty", "/dev/ptmx", "/dev/pts"];

/// Pseudo-filesystems a `shared_paths` grant must never re-admit: `/proc`
/// (issue #31's `/proc/<pid>/environ` env leak) and `/sys`. They are kept out
/// by *omission* from the read set (SPEC-0016/FR-1607), but Landlock's
/// recursive, allow-only grant has no deny primitive — so a shared path equal
/// to `/`, or a lexical ancestor of one of these, would grant the whole tree
/// beneath it and re-open the leak with no way to carve it back out. Such
/// shares are refused.
const PROTECTED_ROOTS: [&str; 2] = ["/proc", "/sys"];

/// Reject a `shared_paths` entry that would re-admit a [`PROTECTED_ROOTS`] tree.
///
/// A share is refused when its normalized form is `/`, is one of the protected
/// roots, or is a lexical ancestor of one — any of which would grant `/proc`
/// (or `/sys`) recursively and defeat the deny-by-omission SPEC-0016/FR-1607
/// relies on. The check is **lexical and injection-pure**: it normalizes the
/// path string only (folding away `.`/`..`/`//`) and performs no
/// `canonicalize`/`stat`/environment access, so it runs and unit-tests on any
/// target. The Linux adapter calls it
/// twice — once on the configured string, and again on the canonicalized path,
/// so a symlink cannot resolve an innocuous-looking entry onto a protected tree.
///
/// # Errors
///
/// Returns [`SandmeError::SharedPathTooBroad`] when the share would grant a
/// protected tree.
pub fn guard_shared_path(path: &Path) -> Result<(), SandmeError> {
    let normalized = normalize_lexical(path);
    let grants_protected = PROTECTED_ROOTS
        .iter()
        .any(|root| Path::new(root).starts_with(&normalized));
    if grants_protected {
        return Err(SandmeError::SharedPathTooBroad {
            path: path.display().to_string(),
        });
    }
    Ok(())
}

/// Lexically normalize a path — fold away `.`, `..` and redundant separators —
/// without touching the filesystem. `/foo/../proc` becomes `/proc`, `/proc/..`
/// becomes `/`. This is a purely syntactic collapse (it says nothing about
/// symlinks), which is exactly why the adapter re-checks the canonicalized path.
fn normalize_lexical(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::Prefix(prefix) => out.push(prefix.as_os_str()),
            Component::RootDir => out.push(Component::RootDir.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(segment) => out.push(segment),
        }
    }
    out
}

/// The injected `HOME`/`XDG_*` environment `build_plan` maps GUI mode against.
///
/// Every field is supplied by the caller — the Linux adapter reads the real
/// process environment, the unit tests pass synthetic values — so `build_plan`
/// itself touches neither the environment nor the filesystem. A missing value
/// is `None`; the XDG mapping falls back to a `HOME`-relative directory
/// (SPEC-0016/FR-1608).
#[derive(Debug, Default, Clone)]
pub struct PlanEnv {
    /// `$HOME`, the base for the `~/.config`/`~/.cache`/`~/.local/share` XDG
    /// fallbacks.
    pub home: Option<PathBuf>,
    /// `$XDG_CONFIG_HOME`, else `~/.config`.
    pub config_home: Option<PathBuf>,
    /// `$XDG_CACHE_HOME`, else `~/.cache`.
    pub cache_home: Option<PathBuf>,
    /// `$XDG_DATA_HOME`, else `~/.local/share`.
    pub data_home: Option<PathBuf>,
    /// `$XDG_RUNTIME_DIR`, the per-user runtime/scratch directory (else `/tmp`).
    pub runtime_dir: Option<PathBuf>,
}

/// The paths and ports a Landlock ruleset should grant, decided from the
/// configuration alone.
///
/// Landlock is allow-only: every grant here is a positive rule, and anything
/// not listed is denied. The Linux adapter turns each field into `landlock`
/// crate calls; nothing in this struct references a `landlock` type, so the
/// decision is reviewable and unit-testable on any target.
#[derive(Debug)]
pub struct AccessPlan {
    /// Paths granted read only.
    pub reads: Vec<PathBuf>,
    /// Paths granted read and execute.
    pub read_execs: Vec<PathBuf>,
    /// Paths granted read and write.
    pub read_writes: Vec<PathBuf>,
    /// TCP ports the child may `connect()` to — the proxy port alone when the
    /// proxy is on (SPEC-0016/FR-1606), or empty when it is off (deny-all-TCP,
    /// SPEC-0017/FR-1706).
    pub connect_ports: Vec<u16>,
}

/// Build the [`AccessPlan`] for one invocation, as a pure function of its
/// inputs (SPEC-0016/NFR-1602).
///
/// `config` supplies `gui_mode`; `shared` is the already-`~`-expanded,
/// symlink-resolved absolute form of `config.shared_paths` (the adapter in
/// [`super`] does that live-FS work, which this module refuses); `read_only` is
/// the same already-resolved, guarded form of `config.read_only_paths`, granted
/// read+execute without write (SPEC-0017/FR-1703); `proxy` is the egress proxy
/// address when one is running — whose port is the only outbound TCP grant — or
/// `None` when the proxy is disabled, yielding an empty allowed-port set that
/// is a fully-enforced deny-all-TCP state (SPEC-0017/FR-1706). `env` is the
/// injected `HOME`/`XDG_*` environment. No filesystem or environment access
/// happens here.
#[must_use]
pub fn build_plan(
    config: &Config,
    shared: &[PathBuf],
    read_only: &[PathBuf],
    proxy: Option<SocketAddr>,
    env: &PlanEnv,
) -> AccessPlan {
    let reads = READ_DIRS.iter().map(PathBuf::from).collect();

    let mut read_execs: Vec<PathBuf> = READ_EXEC_DIRS.iter().map(PathBuf::from).collect();
    // The shell the single-operand form is routed through. `/bin` above would
    // normally cover it, but it is granted explicitly as a safety net: the
    // single-operand path (SPEC-0013/FR-1301) MUST stay executable even if the
    // enumerated read+exec dirs are ever narrowed, so the shell is never left
    // unreachable.
    read_execs.push(PathBuf::from(crate::sandbox::SHELL));
    // Read-only paths join the read+execute bucket (one bucket, read+exec —
    // SPEC-0017/FR-1703): a toolchain prefix must be executable, and execute
    // on a pure-data directory is harmless. They are already resolved and guarded by
    // the adapter through the same path as `shared`.
    read_execs.extend(read_only.iter().cloned());

    let mut read_writes: Vec<PathBuf> = DEVICE_READ_WRITE.iter().map(PathBuf::from).collect();
    read_writes.extend(shared.iter().cloned());
    if config.gui_mode {
        read_writes.extend(xdg_grants(env));
    }

    AccessPlan {
        reads,
        read_execs,
        read_writes,
        // Empty when the proxy is off: the base ruleset still handles
        // ConnectTcp, so an empty port list is deny-all outbound TCP, fully
        // enforced at the ABI-v4 floor (SPEC-0017/FR-1706). Never a sentinel
        // port.
        connect_ports: proxy.map(|p| vec![p.port()]).unwrap_or_default(),
    }
}

/// The XDG user directories GUI mode grants read-write (SPEC-0016/FR-1608).
///
/// `$XDG_CONFIG_HOME`/`$XDG_CACHE_HOME`/`$XDG_DATA_HOME` when set, else the
/// `~/.config`/`~/.cache`/`~/.local/share` fallbacks, plus the per-user
/// runtime/scratch directory (`$XDG_RUNTIME_DIR` else `/tmp`). A fallback is
/// skipped when `$HOME` is unset and no explicit value was given, since there
/// is then no directory to name.
fn xdg_grants(env: &PlanEnv) -> Vec<PathBuf> {
    let mut grants = Vec::new();
    let home = env.home.as_deref();

    grants.extend(xdg_dir(env.config_home.as_deref(), home, ".config"));
    grants.extend(xdg_dir(env.cache_home.as_deref(), home, ".cache"));
    grants.extend(xdg_dir(env.data_home.as_deref(), home, ".local/share"));
    grants.push(
        env.runtime_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from("/tmp")),
    );
    grants
}

/// Resolve one XDG directory: the explicit value if present, else `home` joined
/// with `fallback`, else `None` when neither is available.
fn xdg_dir(explicit: Option<&Path>, home: Option<&Path>, fallback: &str) -> Option<PathBuf> {
    match explicit {
        Some(path) => Some(path.to_path_buf()),
        None => home.map(|home| home.join(fallback)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, SocketAddr};

    /// A proxy address whose only load-bearing part is its port.
    fn proxy(port: u16) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, port))
    }

    /// A config with the given GUI mode and no shared paths of its own.
    fn config_with(gui_mode: bool) -> Config {
        Config {
            shared_paths: Vec::new(),
            read_only_paths: Vec::new(),
            proxy: true,
            proxy_port: 8787,
            gui_mode,
            allow_private_egress: false,
        }
    }

    /// Injected environment naming a synthetic home and no XDG overrides, so the
    /// tests never touch the real environment or filesystem.
    fn env_with_home() -> PlanEnv {
        PlanEnv {
            home: Some(PathBuf::from("/synthetic/home")),
            ..PlanEnv::default()
        }
    }

    #[test]
    fn grants_the_enumerated_system_directories_read_and_execute() {
        let plan = build_plan(
            &config_with(false),
            &[],
            &[],
            Some(proxy(8787)),
            &PlanEnv::default(),
        );

        for dir in ["/usr", "/bin", "/sbin", "/lib", "/lib64", "/opt"] {
            assert!(
                plan.read_execs.contains(&PathBuf::from(dir)),
                "{dir} should be granted read+execute"
            );
        }
    }

    #[test]
    fn grants_execute_on_the_shell() {
        let plan = build_plan(
            &config_with(false),
            &[],
            &[],
            Some(proxy(8787)),
            &PlanEnv::default(),
        );

        assert!(
            plan.read_execs.contains(&PathBuf::from("/bin/bash")),
            "the shell the single-operand form uses must be executable: {:?}",
            plan.read_execs
        );
    }

    #[test]
    fn grants_the_configured_shared_paths_read_write() {
        // A synthetic already-resolved path: the builder takes it as given and
        // never consults the filesystem to confirm it exists.
        let shared = [PathBuf::from("/synthetic/project")];
        let plan = build_plan(
            &config_with(false),
            &shared,
            &[],
            Some(proxy(8787)),
            &PlanEnv::default(),
        );

        assert!(
            plan.read_writes
                .contains(&PathBuf::from("/synthetic/project")),
            "a shared path must be read-write: {:?}",
            plan.read_writes
        );
    }

    #[test]
    fn grants_the_pseudo_terminal_devices_read_write() {
        let plan = build_plan(
            &config_with(false),
            &[],
            &[],
            Some(proxy(8787)),
            &PlanEnv::default(),
        );

        assert!(plan.read_writes.contains(&PathBuf::from("/dev/ptmx")));
        assert!(plan.read_writes.contains(&PathBuf::from("/dev/pts")));
    }

    #[test]
    fn grants_outbound_tcp_only_to_the_proxy_port() {
        let plan = build_plan(
            &config_with(false),
            &[],
            &[],
            Some(proxy(9191)),
            &PlanEnv::default(),
        );

        assert_eq!(plan.connect_ports, vec![9191]);
    }

    #[test]
    fn never_grants_the_root_directory_as_a_whole() {
        // The widest configuration the builder can produce.
        let shared = [PathBuf::from("/synthetic/project")];
        let plan = build_plan(
            &config_with(true),
            &shared,
            &[],
            Some(proxy(8787)),
            &env_with_home(),
        );

        for path in plan
            .reads
            .iter()
            .chain(&plan.read_execs)
            .chain(&plan.read_writes)
        {
            assert_ne!(path, &PathBuf::from("/"), "no rule may grant / as a whole");
        }
    }

    #[test]
    fn never_grants_proc() {
        // /proc is closed by omission (issue #31, SPEC-0016/FR-1607) — but
        // only conditionally: `build_plan` echoes back whatever `shared` it is
        // handed, so the
        // invariant holds because the adapter rejects a `/`- or `/proc`-ancestor
        // share up front via `guard_shared_path` (see `rejects_*` below), never
        // because `build_plan` filters `/proc` itself. Here the share is benign,
        // so no rule may mention /proc.
        let shared = [PathBuf::from("/synthetic/project")];
        let plan = build_plan(
            &config_with(true),
            &shared,
            &[],
            Some(proxy(8787)),
            &env_with_home(),
        );

        for path in plan
            .reads
            .iter()
            .chain(&plan.read_execs)
            .chain(&plan.read_writes)
        {
            assert!(
                !path.starts_with("/proc"),
                "/proc must never be granted, found {path:?}"
            );
        }
    }

    #[test]
    fn withholds_the_xdg_directories_when_gui_mode_is_off() {
        let plan = build_plan(
            &config_with(false),
            &[],
            &[],
            Some(proxy(8787)),
            &env_with_home(),
        );

        for suffix in [".config", ".cache", ".local/share"] {
            let dir = PathBuf::from("/synthetic/home").join(suffix);
            assert!(
                !plan.read_writes.contains(&dir),
                "{dir:?} must not be granted without GUI mode"
            );
        }
    }

    #[test]
    fn grants_the_xdg_fallback_directories_under_gui_mode() {
        let plan = build_plan(
            &config_with(true),
            &[],
            &[],
            Some(proxy(8787)),
            &env_with_home(),
        );

        for suffix in [".config", ".cache", ".local/share"] {
            let dir = PathBuf::from("/synthetic/home").join(suffix);
            assert!(
                plan.read_writes.contains(&dir),
                "GUI mode should grant the XDG fallback {dir:?}: {:?}",
                plan.read_writes
            );
        }
        assert!(
            plan.read_writes.contains(&PathBuf::from("/tmp")),
            "the runtime/scratch dir falls back to /tmp: {:?}",
            plan.read_writes
        );
    }

    #[test]
    fn honours_explicit_xdg_overrides_under_gui_mode() {
        let env = PlanEnv {
            home: Some(PathBuf::from("/synthetic/home")),
            config_home: Some(PathBuf::from("/xdg/config")),
            cache_home: Some(PathBuf::from("/xdg/cache")),
            data_home: Some(PathBuf::from("/xdg/data")),
            runtime_dir: Some(PathBuf::from("/run/user/1000")),
        };
        let plan = build_plan(&config_with(true), &[], &[], Some(proxy(8787)), &env);

        for dir in ["/xdg/config", "/xdg/cache", "/xdg/data", "/run/user/1000"] {
            assert!(
                plan.read_writes.contains(&PathBuf::from(dir)),
                "the explicit XDG value {dir} should win over the fallback: {:?}",
                plan.read_writes
            );
        }
        // And the ~/.config fallback is not also added when the override exists.
        assert!(
            !plan
                .read_writes
                .contains(&PathBuf::from("/synthetic/home/.config")),
            "the fallback must not appear alongside the explicit override"
        );
    }

    #[test]
    fn rejects_a_shared_path_that_would_re_admit_a_protected_tree() {
        // A `/` share and a lexical ancestor of /proc (root, and /proc itself)
        // would each grant /proc recursively and re-open issue #31 — the guard
        // must refuse them. All checks are lexical: no filesystem is touched.
        for entry in ["/", "/proc", "/sys", "/proc/../", "/./"] {
            let error = guard_shared_path(Path::new(entry))
                .expect_err(&format!("{entry} must be rejected as too broad"));
            assert!(
                matches!(error, SandmeError::SharedPathTooBroad { .. }),
                "{entry} should be SharedPathTooBroad, was {error:?}"
            );
        }
    }

    #[test]
    fn accepts_a_shared_path_below_the_protected_trees() {
        // A concrete project directory, and a path merely sharing a name prefix
        // with a protected root (`/proc-data` is not `/proc`), are both fine:
        // the guard compares whole path components, not string prefixes.
        for entry in ["/synthetic/project", "/proc-data", "/home/user/proc"] {
            assert!(
                guard_shared_path(Path::new(entry)).is_ok(),
                "{entry} is below no protected tree and must be accepted"
            );
        }
    }

    #[test]
    fn grants_no_outbound_tcp_when_the_proxy_is_off() {
        // Given the proxy is off (None), the allowed-port set is empty — a
        // deny-all outbound TCP state under the base ConnectTcp handling
        // (SPEC-0017/FR-1706)
        let plan = build_plan(&config_with(false), &[], &[], None, &PlanEnv::default());

        assert!(
            plan.connect_ports.is_empty(),
            "no proxy means no allowed port: {:?}",
            plan.connect_ports
        );
    }

    #[test]
    fn grants_read_only_paths_read_execute_never_read_write() {
        // A synthetic already-resolved read-only path lands in read_execs
        // (read+execute, one bucket — SPEC-0017/FR-1703) and never in
        // read_writes.
        let read_only = [PathBuf::from("/synthetic/toolchain")];
        let plan = build_plan(
            &config_with(false),
            &[],
            &read_only,
            Some(proxy(8787)),
            &PlanEnv::default(),
        );

        assert!(
            plan.read_execs
                .contains(&PathBuf::from("/synthetic/toolchain")),
            "a read-only path must be read+execute: {:?}",
            plan.read_execs
        );
        assert!(
            !plan
                .read_writes
                .contains(&PathBuf::from("/synthetic/toolchain")),
            "a read-only path must never be granted write: {:?}",
            plan.read_writes
        );
    }
}
