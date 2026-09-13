# SPEC-0016: Linux Landlock backend

## Metadata

- **Status**: Draft
- **Created**: 2026-09-13
- **Updated**: 2026-09-13
- **Related ADRs**: none yet
- **Related specs**: SPEC-0015 (fills the `Backend` seam with the Linux backend), SPEC-0001/NFR-001
  (platform flip: no longer macOS-only), SPEC-0003 (egress — weakened to port-only on Linux),
  SPEC-0004 (exit-status contract — preserved without an exec wrapper), SPEC-0005/SPEC-0006
  (process-information isolation — reduced to the `/proc` omission on Linux), SPEC-0009 (PTY — the
  Linux `/dev/pts` re-spec)

## Summary

`sandme` runs today only on macOS: the non-macOS `cfg` arm is a `compile_error!`. This spec adds the
Linux sandbox backend built on the kernel's **Landlock** LSM, so a Linux developer gets the same
"confine the command, route egress through the proxy" guarantee a macOS developer already has. It
also records the contracts Landlock's allow-only model forces to be **weaker** than the macOS
Seatbelt backend — egress by port only, `/proc` denied wholesale, a hard kernel floor — so they are
reviewed here, not discovered later. **Cycle constraint:** development is macOS-only; every
"builds/enforces on Linux" claim is a REVIEW claim (read now, executed by a future CI), not an
executed gate.

## Spec deltas

These record how the Linux backend changes contracts stated for macOS. **No requirement ID in
SPEC-0001/0003/0005/0009 is edited in place** — those IDs are immutable; the deltas live here.

### MODIFIED

- **SPEC-0001 / NFR-001 — platform.** Was: "macOS only; the sandbox is macOS Seatbelt
  (`sandbox-exec`)." Now: sandme supports **macOS and Linux**, selected at compile time
  (`cfg(target_os)`); Linux is Landlock. Backend selection is three arms — macOS → `seatbelt`,
  Linux → `landlock`, every other target → `compile_error!`. macOS behaviour is byte-for-byte
  unchanged (SPEC-0015 NFR-1501).
- **SPEC-0003 — egress.** On Linux the egress grant is **by port only** (Landlock `ConnectTcp` has
  no address predicate); the macOS `localhost:<port>` address pin cannot be reproduced. This is a
  stated security limitation, not a footnote — see "Egress is a weakened contract" below.
- **SPEC-0005 / SPEC-0006 — process-information isolation.** On Linux the coverage of issue #31 is
  reduced to a `/proc` **deny-by-omission**. There is no Landlock analogue for `sysctl` filtering
  (SPEC-0006 `kern.procargs`) or macOS `process-info`; the same-uid `ptrace` read is out of scope.
- **SPEC-0009 — PTY.** SPEC-0009 declined `/dev/pts` as speculative "for a hypothetical Linux port."
  That port now exists, so this spec **re-specs** the PTY grant on Linux as `/dev/ptmx` + `/dev/pts`.

## Problem

`sandbox::run` selects a `Backend` by `cfg(target_os)` (SPEC-0015). The macOS arm is `seatbelt`; the
non-macOS arm is `compile_error!`, so Linux users have **no sandbox at all**. Landlock is not a
drop-in port of Seatbelt:

- **Allow-only union, no deny.** A ruleset is a set of *allowed* accesses; anything not added is
  denied. Every Seatbelt "allow broad, deny a subtree" rule must invert into "grant only the wanted
  subtrees." This is the source of the `/proc` and no-root-grant decisions.
  - **`~/.sandme` within a shared home.** macOS keeps `(deny file-write* ~/.sandme)` even under a
    broad `~` allow, so a sandboxed command cannot rewrite the policy that governs the next run
    (issues #41, #12). Landlock has no such deny: if a `shared_paths` entry contains `~/.sandme`
    (i.e. the user shares their real `$HOME` or an ancestor of it), that config becomes writable.
    In normal use a share names a project directory, not `$HOME`, so `~/.sandme` is protected by
    omission; the guarantee is lost only under an explicit home-or-wider share. The integration
    test that asserts the carve-out is therefore macOS-only. Closing this on Linux needs the
    enumerate-and-omit approach (grant a shared dir's entries except `.sandme`), deferred with the
    other deny-inversion follow-ups.
- **Applied to the process, not via a wrapper.** There is no `sandbox-exec`. The child restricts
  *itself* with `restrict_self()` after `fork` and before `exec`; the ruleset (with its open path
  descriptors) is built in the parent.
- **Versioned by kernel ABI.** Filesystem rules exist from Linux 5.13 (ABI v1); the outbound
  **TCP-port** rule (`ConnectTcp`) requires Linux 6.7 (ABI v4). Below v4 the network cannot be
  restricted at all.
- **Coarser filters.** Outbound TCP is filtered by **port number only** — no host/IP. There is no
  `sysctl`, `process-info`, or AppleEvents analogue.

## Goals

- A Landlock backend that satisfies the existing `Backend` trait with no change to the trait,
  `sandbox::run`, or the `seatbelt` backend.
- **Fail-shut** parity with Seatbelt's all-or-nothing guarantee: refuse to run rather than run
  unconfined or with egress unrestricted.
- A ruleset **decision layer** (`plan.rs`) that is a pure function of injected inputs, so it
  compiles and unit-tests on macOS without a Linux kernel.
- Every weakened contract recorded here (egress, `/proc`, kernel floor, PTY, exit status), each with
  the follow-up that would close it.

## Non-goals

- Standing up a GitHub Actions CI matrix or executing `cargo build`/`test` on a real Linux kernel
  (deferred; no CI workflow is created).
- A bubblewrap/firejail wrapper backend (weighed and rejected in design D0 — Landlock needs no extra
  host binary and works where unprivileged user namespaces are disabled).
- Implementing the network-namespace egress filter or a narrow `/proc/self` grant this cycle (named
  follow-ups below).
- Editing SPEC-0001/0003/0005/0009 in place.

## User scenarios *(mandatory)*

### Story 1 — A Linux developer gets the same protection (P1)

As a Linux developer, I want `sandme <tool>` to confine the tool's filesystem access and route its
egress through the proxy, the same way it does on macOS.

**Acceptance scenarios**

1. **Given** a Linux kernel that supports Landlock ABI v4, **When** a command runs under sandme,
   **Then** it can read/write only the granted paths and can open outbound TCP only to the proxy
   port.
2. **Given** a command that writes outside `shared_paths`, **When** it runs, **Then** the kernel
   denies the write.

### Story 2 — An unenforceable kernel fails shut, not open (P1)

As a security-conscious user, I want sandme to **refuse** rather than run my command unsandboxed when
the kernel cannot enforce the sandbox.

**Acceptance scenarios**

1. **Given** a kernel where Landlock is absent, disabled, or below ABI v4, **When** sandme is
   invoked, **Then** it refuses to run and reports that the sandbox cannot be enforced (exit
   `125`).

### Story 3 — The weakened contracts are visible, not surprising (P1)

As a reviewer, I want the places Linux is weaker than macOS written down, so I can judge the residual
risk instead of discovering it.

**Acceptance scenarios**

1. **Given** this spec, **When** I read the egress, `/proc`, kernel-floor and exit-status sections,
   **Then** each states its limitation and the named follow-up that closes it.

### Edge cases

- A `shared_paths` entry that does not resolve (missing directory): dropped from the ruleset — a
  hierarchy with no descriptor cannot be granted.
- A fixed system directory absent on this host (`/lib64`, `/opt`): skipped, not fatal.
- A program on the child's `PATH` but **outside** the read+execute grant set: passes the pre-spawn
  resolution, then the kernel denies its exec — an opaque start failure, not 126/127 (recorded
  below).

## Requirements *(mandatory)*

### Functional

- **FR-1601**: WHEN compiled for `target_os = "linux"` THE SYSTEM SHALL select a Landlock backend
  satisfying the `Backend` contract, without changing the trait, `sandbox::run`, or the `seatbelt`
  backend; selection SHALL remain a compile-time three-arm `cfg(target_os)` choice (macOS, Linux,
  `compile_error!`) with no runtime dispatch.
- **FR-1602**: THE SYSTEM SHALL build the Landlock ruleset — the ruleset and every path/network rule
  — in the parent process before spawning, and apply it to the child by calling only `restrict_self`
  after `fork` and before `exec`; the pre-exec step SHALL do nothing that is not async-signal-safe.
- **FR-1603**: IF applying the restriction in the child fails, or the restriction is not fully
  enforced, THEN THE SYSTEM SHALL prevent the command from exec-ing and report a start failure — it
  SHALL NOT run the command unrestricted. (The child-side `RestrictionStatus` check under
  `CompatLevel::HardRequirement` is the load-bearing guarantee; the parent ABI probe below is a
  diagnostic only.)
- **FR-1604**: THE SYSTEM SHALL run a command on Linux only when the kernel can enforce **both** the
  filesystem restriction and the TCP-port egress restriction. IF the kernel's Landlock ABI is below
  **v4** (Linux 6.7), absent, or disabled, THEN THE SYSTEM SHALL refuse to run and report why
  (`SandmeError::SandboxNotEnforced`, exit `125`), rather than confine the filesystem while leaving
  egress unrestricted. Partial enforcement SHALL NOT be silently accepted.
- **FR-1605**: THE SYSTEM SHALL confine the command's filesystem access to an enumerated allowlist:
  read+execute on the fixed system directories a command needs to start and run (including
  `/bin/bash`), read-only on configuration/certificate and random-device paths, and read-write on
  each `shared_paths` entry with `~` expanded and symlinks resolved. It SHALL express every "deny a
  subtree" intent as omission and SHALL NOT grant `/` as a whole.
- **FR-1606**: THE SYSTEM SHALL permit outbound TCP only to the proxy's port
  (`NetPort::new(proxy_port, ConnectTcp)`), and grant no other outbound TCP.
- **FR-1607**: THE SYSTEM SHALL NOT grant `/proc` (issue #31): the omission — not a deny rule — is
  what keeps `/proc/<pid>/environ` unreadable.
- **FR-1608**: WHERE `gui_mode` is enabled THE SYSTEM SHALL grant read-write on the XDG user
  directories (`$XDG_CONFIG_HOME`/`$XDG_CACHE_HOME`/`$XDG_DATA_HOME` else `~/.config`/`~/.cache`/
  `~/.local/share`) and the per-user runtime/scratch dir (`$XDG_RUNTIME_DIR` else `/tmp`), and grant
  none of them when it is disabled.
- **FR-1609**: THE SYSTEM SHALL grant read-write on `/dev/ptmx` and the `/dev/pts` subtree so a
  command can allocate and drive a pseudo-terminal (superseding SPEC-0009's Linux decline).
- **FR-1610**: THE SYSTEM SHALL preserve SPEC-0004's contract without an exec wrapper: `exec_failure`
  SHALL return `None`, and `command()` SHALL resolve the direct-exec program before spawning
  (`executable::locate`/`exec_failure`), returning `CommandNotFound` (127) / `CommandNotExecutable`
  (126) before the child starts. A signal death SHALL still surface as `128 + n`; a normal exit
  passes through unchanged.

### Non-functional

- **NFR-1601**: macOS behaviour SHALL be byte-for-byte unchanged (SPEC-0015 NFR-1501): the macOS
  integration suite passes unchanged except for the `#[cfg(target_os = "macos")]` gating, with no
  assertion weakened.
- **NFR-1602**: The ruleset decision layer (`plan.rs`) SHALL be `cfg`-neutral and injection-pure —
  a pure function of `&Config` + proxy `SocketAddr` + injected `HOME`/`XDG_*`, with **no**
  live-filesystem canonicalization — so it unit-tests on macOS; the `~`/symlink resolution of
  `shared_paths` lives in the Linux-only adapter.
- **NFR-1603**: The `landlock`/`libc` dependencies SHALL be target-scoped
  (`[target.'cfg(target_os = "linux")'.dependencies]`), so nothing new compiles on macOS.

## Stated security limitations *(mandatory)*

These are **contracts**, not caveats. Each names the follow-up that closes it.

### Egress is a weakened contract — port only, not address (SPEC-0003, D4/R1)

Landlock filters outbound TCP by **port number only**, with no host/IP predicate. sandme cannot
reproduce the macOS rule `(allow network-outbound (remote ip "localhost:<port>"))`, which pins both
address and port. Because the proxy port is the **sole** open outbound port and the child is handed
it via `HTTP(S)_PROXY`, a command inside the sandbox is not obliged to send its bytes *through* the
proxy: it can `connect(attacker_host, proxy_port)` directly — to any host listening on that same port
number — and the kernel permits it, because the rule keys on the port alone. That round-trips **past
the proxy's egress policy and its credential entirely**. The macOS backend does **not** have this
hole: it pins `localhost:<port>`, so the same-port-different-host destination is unreachable. This is
a live exfiltration path on Linux, recorded as a stated security limitation. **Follow-up:** a
loopback-only network namespace for the child, pinning egress to `127.0.0.1:<port>` the way Seatbelt
does (see the kernel-floor follow-up below — the same netns work closes both).

### `/proc` is denied by omission; the ptrace read is out of scope (SPEC-0005/0006, D5)

Omitting `/proc` closes the `/proc/<pid>/environ` env-leak of issue #31. Two residuals are recorded:

- **Same-uid `ptrace` is not closed.** Landlock mediates filesystem access, not `ptrace`. A same-uid
  sandboxed process can `PTRACE_ATTACH` a sibling and read its environment straight from memory — and
  it need not be a descendant, so Yama `ptrace_scope=1` (which still permits ptracing one's own
  descendants) does not stop it. This requirement is scoped to "unreadable **via `/proc`**"; a
  filesystem LSM cannot close the ptrace vector and this spec makes no claim to.
- **No `sysctl`/`kern.procargs` analogue.** SPEC-0006's `sysctl` filtering and macOS `process-info`
  have **no Landlock equivalent at all**; on Linux the #31 coverage reduces to the `/proc` omission.
- **Compatibility cost.** Omitting `/proc` also withholds the command's own `/proc/self`, which some
  tooling reads — a stricter posture than macOS. **Follow-up:** a future narrow, per-process
  `/proc/self` grant if a safe mechanism is found.

### PTY ioctls are unrestricted on granted devices (SPEC-0009, D6)

Because sandme does not require ABI v5, Landlock does not restrict `ioctl` on device files, so
`TIOCSCTTY`/`TIOCSTI` need no dedicated grant — and, as on macOS, cannot be filtered by request.
Recorded residual: ioctls are unrestricted on the granted PTY devices.

### The exit-status contract holds only through the granted directories (SPEC-0004, D7)

`executable::locate` resolves the program using sandme's own **unsandboxed** `PATH`/FS view, while
the child may exec only inside the read+execute grant set (FR-1605). To keep 126/127 meaningful, that
enumeration MUST cover the directories the child's `PATH` reaches, so "resolvable" and "executable
under Landlock" describe the same set. **Residual:** a `PATH` directory deliberately left outside the
grant set still fails **opaquely** — a Landlock exec denial surfacing as `SandmeError::Execute`, not
126 — so the contract holds only for programs reached through the granted directories.

## Named follow-up — lower the floor and pin the egress address (D3/D4/R6)

The **ABI v4 floor is a choice, not an inherent Linux limit.** It follows only from making Landlock
the *sole* egress mechanism: `ConnectTcp` is what needs 6.7, so requiring it draws the cliff.
Filesystem confinement alone is available from 5.13 (ABI v1), and the v4 floor needlessly excludes
widely deployed LTS kernels — **Debian 12 (6.1), Ubuntu 22.04 (5.15), RHEL 9 (5.14)**. This cycle
**keeps** the v4 floor and fails shut below it (correct while Landlock is the only egress control).

The follow-up that closes both the floor and the egress limitation: pair **FS-Landlock v1 (5.13+)**
with a **network-namespace + nftables egress filter**. That combination would (a) lower the kernel
floor to 5.13, admitting the excluded LTS kernels, and (b) pin egress to the loopback **address**
(`127.0.0.1:<port>`) rather than a bare port, closing the same-port-different-host bypass above. It
is out of scope this cycle (the "implement netns this cycle" inversion was rejected in design), and
recorded here as a follow-up only.

## Interface contract

No CLI or configuration-key surface changes. `gui_mode` stays a single cross-platform flag; only the
directories it maps to differ per platform.

**Exit codes / errors**

| Code | Condition | Message to user |
| --- | --- | --- |
| `125` | `SandboxNotEnforced` — kernel below the ABI-v4 floor, or Landlock absent/disabled | `sandme: this kernel cannot enforce the sandbox: <reason>; refusing to run the command` |
| `126` | program found but not executable (resolved before spawn) | `sandme: <cmd>: found but not executable` |
| `127` | program not found (resolved before spawn) | `sandme: <cmd>: command not found` |

**Module layout**

```
src/sandbox/landlock/
  mod.rs    # mod plan; (unconditional) + #[cfg(linux)] Landlock, impl Backend, pre_exec, ABI probe,
            #   live-FS ~/symlink resolution of shared_paths, PlanEnv from the real environment
  plan.rs   # cfg-neutral, injection-pure AccessPlan builder (unit-tested on macOS)
```

## Key entities *(optional)*

- **`AccessPlan`**: the decided grants — `reads`, `read_execs`, `read_writes` (each `Vec<PathBuf>`)
  and `connect_ports` (`Vec<u16>`). Holds no `landlock` type, so it is reviewable and testable on any
  target.
- **`PlanEnv`**: the injected `HOME`/`XDG_*` values the GUI mapping reads, so the decision layer
  never touches the real environment.

## Constraints and dependencies

- Landlock LSM; the `landlock` crate (ABI up to at least v4) and `libc`, both **target-scoped** to
  Linux, added in the task that first uses them (KB Rust guideline).
- `restrict_self` runs in `Command::pre_exec` after `fork`, so it must be async-signal-safe: the
  ruleset and all `PathFd` opens happen in the parent; the closure allocates nothing and reports
  failure as an errno-based `io::Error`.
- The `Backend` trait, `sandbox::run` orchestration and the `seatbelt` backend are unchanged
  (SPEC-0015 NFR-1501).

## Success criteria *(mandatory)*

- **SC-1601**: On macOS the full suite passes unchanged (same assertions), and `plan.rs`'s unit
  tests pass — the decision layer is exercised without a Linux kernel.
- **SC-1602**: `cargo fmt`/`clippy`/`build`/`test`/`doc` are clean on macOS with no new dependency
  compiled there.
- **SC-1603 (REVIEW)**: the Linux backend reads correctly against this spec — fail-shut via the
  child `RestrictionStatus` check under `HardRequirement`; egress `ConnectTcp` on the proxy port
  only; `/proc` omitted; `exec_failure` returns `None` with pre-spawn resolution. Executed on a real
  6.7+ kernel by a future CI (deferred).

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-1601, NFR-1601 | the three-arm `cfg` selection compiles; the macOS suite passes unchanged (gated only by `cfg`) |
| FR-1605, FR-1606, FR-1607, FR-1608, FR-1609 | `plan.rs` unit tests assert the rule set, no `/` grant, `/proc` absent, the proxy port, the PTY devices, and the XDG mapping (on/off + override) |
| FR-1602, FR-1603, FR-1604, FR-1610 | REVIEW — the ruleset/`pre_exec`/ABI-probe/pre-spawn-resolution code reads correctly against the design; executed on a 6.7+ kernel by a future CI |
| NFR-1602, NFR-1603 | `plan.rs` takes injected env and does no FS I/O; `Cargo.toml`'s Landlock deps are target-scoped |

## Assumptions

- **Deferred-CI verification gate (async-signal-safety of `restrict_self`).** That `restrict_self`
  itself performs no post-fork heap allocation or lock acquisition in the child is **not confirmed
  this cycle** — the spike was not performed (landlock 0.4.x never compiled here), so it must not be
  treated as done. A future CI MUST confirm this before the backend ships. Mitigation already in
  place: sandme installs no global logger, so the `log`-crate reentrancy vector (a logging call
  allocating/locking after `fork`) is defanged regardless.
- **Deferred-CI build note (toolchain-unverified assumptions).** The `libc` syscall constants
  (`SYS_landlock_create_ruleset`, `LANDLOCK_CREATE_RULESET_VERSION`) and the landlock-API assumptions
  (`from_read` including execute, the `RulesetStatus`/`no_new_privs` fields of `RestrictionStatus`,
  the `default-features = false` feature set still exposing the core API) are **unverified against a
  real toolchain** — they are build-time claims a future CI executes, not confirmed this cycle.
- The child's `PATH` reaches only directories inside the enumerated read+execute grant set for the
  126/127 contract to fire; a `PATH` entry outside it is the recorded opaque residual.

## Open questions

- None that affect the specs, the approach, or the tasks. The remaining unknowns are executed
  verifications the cycle constraint defers (does it build and enforce on a real 6.7+ kernel), which
  a future CI resolves without changing anything decided here.

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-13 | Initial draft, recording the Linux Landlock backend, its fail-shut posture, and the weakened-contract deltas (egress port-only, `/proc` omission, PTY, exit status). |
