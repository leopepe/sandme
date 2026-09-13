//! The Linux sandbox backend: a Landlock ruleset the child applies to itself.
//!
//! Landlock is an allow-only union of path and TCP-port rules with no deny
//! primitive, applied to the process after `fork` and before `exec` — there is
//! no `sandbox-exec` wrapper (unlike the macOS `seatbelt` backend). The policy
//! decision (which paths, rights and ports) lives in the `cfg`-neutral [`plan`]
//! module, so it compiles and unit-tests on macOS. Everything that needs a
//! Linux kernel — the `landlock` crate calls, the `pre_exec` glue, the ABI
//! probe, and the live-filesystem `~`/symlink resolution of `shared_paths` —
//! lives in the Linux-only `backend` module below.

// `plan` is consumed by the Linux `backend` below and by its own unit tests. On
// every other target the `backend` is `cfg`'d out, so a non-test build has no
// live caller and would flag the whole layer as dead — allow that off Linux,
// while keeping real dead-code detection on the platform that uses it.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
mod plan;

#[cfg(target_os = "linux")]
pub use backend::Landlock;

#[cfg(target_os = "linux")]
mod backend {
    use std::io;
    use std::net::SocketAddr;
    use std::os::unix::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::ExitStatus;

    use landlock::{
        ABI, Access, AccessFs, AccessNet, BitFlags, CompatLevel, Compatible, NetPort, PathBeneath,
        PathFd, Ruleset, RulesetAttr, RulesetCreated, RulesetCreatedAttr, RulesetError,
        RulesetStatus,
    };

    use super::plan::{self, AccessPlan, PlanEnv};
    use crate::config::Config;
    use crate::error::SandmeError;
    use crate::executable;
    use crate::sandbox::Backend;

    /// The required Landlock ABI floor: v4 (Linux 6.7).
    ///
    /// v1 (Linux 5.13) can restrict the filesystem; `ConnectTcp` — the outbound
    /// TCP-port restriction SPEC-0003's egress guarantee needs — arrives with v4
    /// (D3). Below v4, sandme cannot confine egress and so fails shut rather than
    /// confine the filesystem while leaving the network open.
    const ABI_FLOOR: i32 = 4;

    /// `LANDLOCK_CREATE_RULESET_VERSION` from `linux/landlock.h` — the flag that
    /// turns `landlock_create_ruleset` into an ABI probe. The `libc` crate does
    /// not export it; the value is a stable part of the kernel UABI.
    const LANDLOCK_CREATE_RULESET_VERSION: u32 = 1;

    /// The ABI whose rights sandme requires. Rights above it (e.g. v5's
    /// `IoctlDev`) are left `BestEffort`, so a 6.7–6.9 kernel is still fully
    /// enforceable and PTY ioctls need no dedicated grant (D3, D6).
    const REQUIRED_ABI: ABI = ABI::V4;

    /// The Linux backend.
    pub struct Landlock;

    impl Backend for Landlock {
        fn command(
            &self,
            program: &str,
            args: &[String],
            config: &Config,
            proxy: SocketAddr,
        ) -> Result<tokio::process::Command, SandmeError> {
            // Parent probe (D2, step 1) — a clean, up-front diagnostic when the
            // kernel is too old. It is only a diagnostic; the child-side check
            // below is the guarantee.
            enforce_abi_floor()?;

            // Preserve SPEC-0004's exit-status contract without an exec wrapper
            // (D7): resolve the program against sandme's own environment before
            // spawning, so a missing/non-executable program is 127/126 here
            // rather than an opaque start failure. `/bin/bash` (the
            // single-operand target) is present, so only the multi-operand
            // direct-exec path can trip this.
            if let Some(failure) = executable::exec_failure(program) {
                return Err(failure);
            }

            // Read the environment once into the injected `PlanEnv`, then reuse
            // its `home` for the `~/` expansion of `shared_paths` — no separate
            // `$HOME` read for tilde expansion (the last direct one is gone).
            let env = plan_env();
            let shared = resolve_shared(&config.shared_paths, env.home.as_deref())?;
            let access = plan::build_plan(config, &shared, proxy, &env);
            let ruleset = create_ruleset(&access)?;

            // The ruleset — and every PathFd inside it — is built here in the
            // parent; the child's pre_exec closure only enforces it.
            let mut ruleset = Some(ruleset);
            let mut command = std::process::Command::new(program);
            command.args(args);
            // SAFETY: the closure runs after `fork` and before `exec`, where only
            // async-signal-safe work is allowed. It allocates nothing: the
            // ruleset is already built, `Option::take` and `restrict_self` do no
            // heap allocation, and a failure is reported as an errno-based
            // `io::Error` (never an allocating `io::Error::new(_, String)`). This
            // is the load-bearing fail-shut check (D2, step 2) — anything short
            // of full enforcement returns `EPERM`, so the command never execs
            // unconfined.
            unsafe {
                command.pre_exec(move || {
                    let ruleset = ruleset
                        .take()
                        .ok_or_else(|| io::Error::from_raw_os_error(libc::EPERM))?;
                    enforce(ruleset)
                });
            }
            Ok(tokio::process::Command::from(command))
        }

        /// Returns `None` for every finished status (D7): with no exec wrapper
        /// there is no sentinel to decode, so the not-found/not-executable
        /// determination is made before spawning (see [`Self::command`]).
        fn exec_failure(&self, _program: &str, _status: ExitStatus) -> Option<SandmeError> {
            None
        }
    }

    /// Apply the already-built ruleset to the current (post-fork) process, and
    /// fail shut unless it is fully enforced AND `no_new_privs` was set.
    ///
    /// `no_new_privs` is what stops the confined child from regaining
    /// privileges through a setuid/setgid exec; without it a fully-enforced FS
    /// ruleset could still be shed. Requiring both is cheap defense-in-depth —
    /// any state short of "fully enforced with no_new_privs" returns `EPERM`, so
    /// the command never execs anything less than fully confined.
    fn enforce(ruleset: RulesetCreated) -> io::Result<()> {
        let status = ruleset
            .restrict_self()
            .map_err(|_| io::Error::from_raw_os_error(libc::EPERM))?;
        match (status.ruleset, status.no_new_privs) {
            (RulesetStatus::FullyEnforced, true) => Ok(()),
            (RulesetStatus::FullyEnforced, false)
            | (RulesetStatus::PartiallyEnforced | RulesetStatus::NotEnforced, _) => {
                Err(io::Error::from_raw_os_error(libc::EPERM))
            }
        }
    }

    /// Refuse up front on a kernel below the ABI floor (D2/D3).
    fn enforce_abi_floor() -> Result<(), SandmeError> {
        let supported = supported_abi();
        if supported >= ABI_FLOOR {
            return Ok(());
        }
        let reason = if supported < 1 {
            "the kernel does not support Landlock, or it is disabled".to_string()
        } else {
            format!(
                "the kernel supports Landlock ABI v{supported}, but v{ABI_FLOOR} \
                 (Linux 6.7, needed to restrict outbound TCP by port) is required"
            )
        };
        Err(SandmeError::SandboxNotEnforced { reason })
    }

    /// The highest Landlock ABI the running kernel supports, or a value below 1
    /// when Landlock is unavailable.
    fn supported_abi() -> i32 {
        // SAFETY: `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)`
        // is the kernel's documented ABI-probe form. With a null attribute
        // pointer and size 0 it creates no ruleset and reads or writes no memory,
        // returning the supported ABI version (>= 1) or -1 with errno set.
        let version = unsafe {
            libc::syscall(
                libc::SYS_landlock_create_ruleset,
                std::ptr::null::<libc::c_void>(),
                0_usize,
                LANDLOCK_CREATE_RULESET_VERSION,
            )
        };
        i32::try_from(version).unwrap_or(-1)
    }

    /// Build the ruleset from the plan, in the parent, with `HardRequirement` so
    /// the crate refuses to silently downgrade on an old kernel (D2, step 2).
    fn create_ruleset(access: &AccessPlan) -> Result<RulesetCreated, SandmeError> {
        let created = base_ruleset().map_err(rule_error)?;
        // Read-only: config, certs and the random devices a command reads but
        // never executes. Read+execute: the enumerated system dirs and the
        // shell. Read+write: shared paths, the PTY devices and (under GUI mode)
        // the XDG dirs.
        let created = add_paths(
            created,
            &access.reads,
            AccessFs::ReadFile | AccessFs::ReadDir,
        )?;
        let created = add_paths(
            created,
            &access.read_execs,
            AccessFs::from_read(REQUIRED_ABI),
        )?;
        let created = add_own_executable(created)?;
        let created = add_paths(
            created,
            &access.read_writes,
            AccessFs::from_all(REQUIRED_ABI),
        )?;
        add_ports(created, &access.connect_ports)
    }

    /// Grant read+execute on sandme's own binary. The git-over-SSH tunnel
    /// (issue #33) re-execs sandme as ssh's `ProxyCommand`, so the confined
    /// child must be able to execute sandme itself — and the binary lives
    /// outside the enumerated system directories. If sandme cannot locate its
    /// own path the grant is skipped; the tunnel simply will not start, which is
    /// the same outcome the `wire_git_ssh` fallback already produces.
    fn add_own_executable(created: RulesetCreated) -> Result<RulesetCreated, SandmeError> {
        match std::env::current_exe() {
            Ok(exe) => add_paths(
                created,
                std::slice::from_ref(&exe),
                AccessFs::from_read(REQUIRED_ABI),
            ),
            Err(_) => Ok(created),
        }
    }

    /// The empty ruleset that handles both the filesystem and the egress rights,
    /// as a hard requirement.
    fn base_ruleset() -> Result<RulesetCreated, RulesetError> {
        Ruleset::default()
            .set_compatibility(CompatLevel::HardRequirement)
            .handle_access(AccessFs::from_all(REQUIRED_ABI))?
            .handle_access(AccessNet::ConnectTcp)?
            .create()
    }

    /// Add one `access` grant for every path in `paths`, skipping any path that
    /// cannot be opened (a fixed directory absent on this host, or a
    /// `shared_paths` entry that does not resolve, is simply not granted). The
    /// three grant kinds — read-only, read+execute, read+write — differ only in
    /// the `AccessFs` bits their callers pass, so they share this one body.
    fn add_paths(
        mut created: RulesetCreated,
        paths: &[PathBuf],
        access: BitFlags<AccessFs>,
    ) -> Result<RulesetCreated, SandmeError> {
        let file_access = AccessFs::from_file(REQUIRED_ABI);
        for path in paths {
            let Ok(fd) = PathFd::new(path) else { continue };
            // Directory-only rights (ReadDir, MakeDir, …) are illegal on a
            // regular file or device node; under HardRequirement the crate
            // rejects such a rule outright. Narrow the grant to the
            // file-legitimate rights for any non-directory path (e.g.
            // /bin/bash, /dev/null, /dev/ptmx).
            let effective = if path.is_dir() {
                access
            } else {
                access & file_access
            };
            if effective.is_empty() {
                continue;
            }
            created = created
                .add_rule(PathBeneath::new(fd, effective))
                .map_err(rule_error)?;
        }
        Ok(created)
    }

    /// Add the outbound TCP-port rules — the proxy port alone (D4).
    fn add_ports(
        mut created: RulesetCreated,
        ports: &[u16],
    ) -> Result<RulesetCreated, SandmeError> {
        for &port in ports {
            created = created
                .add_rule(NetPort::new(port, AccessNet::ConnectTcp))
                .map_err(rule_error)?;
        }
        Ok(created)
    }

    /// Map a `landlock` ruleset-construction failure to sandme's fail-shut error:
    /// if the ruleset cannot be built, the command must not run.
    fn rule_error(error: RulesetError) -> SandmeError {
        SandmeError::SandboxNotEnforced {
            reason: format!("the Landlock ruleset could not be built: {error}"),
        }
    }

    /// Resolve each `shared_paths` entry's `~` and symlinks to an absolute real
    /// path — the live-filesystem work [`plan`] deliberately refuses (D1/D3).
    ///
    /// A path that cannot be resolved (it does not exist) is dropped: Landlock
    /// cannot grant a hierarchy that has no descriptor to open.
    ///
    /// Each entry is run through [`plan::guard_shared_path`] twice: once on the
    /// configured string (a lexical, injection-pure reject of a `/`- or
    /// `/proc`/`/sys`-ancestor share), and again on the canonicalized path, so a
    /// symlink cannot resolve an innocuous-looking entry onto a protected tree
    /// (F1/D5). Either rejection fails the whole invocation shut.
    fn resolve_shared(paths: &[String], home: Option<&Path>) -> Result<Vec<PathBuf>, SandmeError> {
        let mut resolved = Vec::with_capacity(paths.len());
        for path in paths {
            plan::guard_shared_path(Path::new(path))?;
            let Some(real) = resolve_one(path, home) else {
                continue;
            };
            plan::guard_shared_path(&real)?;
            resolved.push(real);
        }
        Ok(resolved)
    }

    /// Resolve one entry: expand a leading `~/` against the injected `home`,
    /// then canonicalize; `None` if it does not resolve.
    fn resolve_one(path: &str, home: Option<&Path>) -> Option<PathBuf> {
        std::fs::canonicalize(expand_tilde(path, home)).ok()
    }

    /// Expand a leading `~/` against the injected `home` — taken from the same
    /// [`PlanEnv`] the adapter already reads for its access plan — leaving
    /// anything else unchanged. Reads no process environment of its own.
    fn expand_tilde(path: &str, home: Option<&Path>) -> PathBuf {
        // `home` carries `plan_env()`'s `var_os` (OsString) semantics, inherited
        // from `PlanEnv`: intentional, so a non-UTF-8 `$HOME` expands the tilde
        // rather than dropping it (widening toward the user's configured share).
        if let Some(rest) = path.strip_prefix("~/")
            && let Some(home) = home
        {
            return home.join(rest);
        }
        PathBuf::from(path)
    }

    /// Read the real `HOME`/`XDG_*` environment into an injected [`PlanEnv`], so
    /// the decision layer stays a pure function of it.
    fn plan_env() -> PlanEnv {
        PlanEnv {
            home: std::env::var_os("HOME").map(PathBuf::from),
            config_home: std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from),
            cache_home: std::env::var_os("XDG_CACHE_HOME").map(PathBuf::from),
            data_home: std::env::var_os("XDG_DATA_HOME").map(PathBuf::from),
            runtime_dir: std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from),
        }
    }
}
