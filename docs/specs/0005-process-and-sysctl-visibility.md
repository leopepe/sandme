# SPEC-0005: Process and kernel-state visibility

## Metadata

- **Status**: Review
- **Created**: 2026-09-09
- **Updated**: 2026-09-09
- **Related ADRs**: none yet
- **Related specs**: SPEC-0001 (extends; adds requirements SPEC-0001 never stated),
  SPEC-0003 (protects the credential FR-203 publishes into the child's environment)

## Summary

sandme's Seatbelt profile grants the sandboxed command `(allow sysctl-read)` and
`(allow process-info-pidinfo)` without restriction. Together those two grants let it read
`KERN_PROCARGS2` for any process of the same uid, which returns that process's full
**environment** — every API key, token and credential the user passed to any other program
they are running. This spec states which kernel state and which processes a sandboxed
command may see: an allowlist of sysctl names, and process information for its own sandbox
instance only.

## Problem

sandme exists to run a coding agent the user does not fully trust
([#31](https://github.com/leopepe/sandme/issues/31)). Today such an agent can read the
environment of every other process the user is running, and exfiltrate what it finds
through the proxy's permitted public egress — the sanctioned route, working as designed.

Reproduced on macOS 26 (Darwin 25.6.0, Apple silicon) with a 30-line C probe **compiled
inside the sandbox**, so the attack needs no privileged setup — a coding agent with a
compiler is enough:

```
$ sandme "$S/envsteal $HOLDER"
LEAKED: HOSTPROC_API_KEY=sk-supersecret-abc123
```

Three properties make this worse than it looks:

- `ps -E` *is* blocked (it is setgid, so the sandbox refuses to execute it), which makes
  the profile look tighter than it is. The kernel interface `ps` would have used is open.
- The read is not covered by any denial the profile states. `(deny default)` does not
  reach it: with the two blanket grants above removed and nothing else changed, the read
  still succeeds. Only an explicit `(deny process-info*)` refuses it.
- It weakens SPEC-0003. That spec's FR-203 requires a per-invocation credential on every
  proxy request, and FR-006 publishes it in the child's environment (`HTTP_PROXY`). A
  concurrent sandme run's credential is therefore readable by any sandboxed command — the
  credential stops being per-invocation.

Neither grant appears in any spec. SPEC-0001 requires a sandbox (FR-002) and filesystem
grants driven by configuration (FR-004); it never says what kernel state or which
processes the command may see, so neither behaviour can be reviewed against a contract.
This spec is the process-and-kernel analogue of what SPEC-0002 did for filesystem grants.

## Goals

- Deny a sandboxed command the environment and arguments of every process outside its own
  sandbox instance, including those of another concurrent sandme run.
- State which sysctl names a sandboxed command may read, and deny the rest.
- Keep the shipped recipes and ordinary development tooling working, measured rather than
  assumed.

## Non-goals

- Auditing the profile's other unrestricted operations — `process-exec`, `mach-lookup`,
  `mach-register`, `mach-bootstrap`, `iokit-open`, `lsopen`, `ipc-posix-shm*`,
  `file-read-metadata`. They are the same class of finding
  ([#12](https://github.com/leopepe/sandme/issues/12) part 4) and each needs its own
  measurement. This spec covers the two operations that carry the environment leak.
- Removing the proxy credential from the child's environment. FR-006 puts it there and
  SPEC-0003 depends on it; this spec removes the cross-process read that exposed it, and
  leaves the transport alone.
- Making the sysctl allowlist configurable. A user-widened allowlist is a way to hand the
  leak back, and no requirement asks for one.
- Hiding a sandboxed command's own environment from itself, or from its own children.
  Inside one sandbox instance there is no boundary to cross.
- Blocking process *enumeration* (`kern.proc.all`, pids and process names). It carries no
  credentials; the allowlist happens to exclude it, and this spec does not require that.

## User scenarios *(mandatory)*

### Story 1 — A sandboxed agent cannot read the host's secrets (P1)

As someone running a coding agent under sandme, I want the agent unable to read what my
other programs hold in their environment, so that a compromised or over-curious agent
cannot collect my API keys and send them out through the proxy.

**Acceptance scenarios**

1. **Given** a process outside the sandbox whose environment contains `SECRET=…`, **When**
   a sandboxed program asks the kernel for that process's arguments and environment,
   **Then** the kernel refuses and the value never reaches the sandboxed program.
2. **Given** the same probe run outside the sandbox, **When** it asks for the same
   process, **Then** it succeeds — the refusal in scenario 1 is the sandbox's doing and
   not the kernel hiding the environment for its own reasons.
3. **Given** two concurrent sandme runs, **When** the command in one asks for the
   environment of the other run's process, **Then** the kernel refuses, so the proxy
   credential SPEC-0003/FR-203 requires stays private to its own run.

### Story 2 — The tools people actually sandbox still work (P1)

As a sandme user, I want the narrower profile to change nothing about how my editor,
compilers and agents behave, so that the security fix costs me no working recipe.

**Acceptance scenarios**

1. **Given** any of the shipped recipes (Neovim, Zed) or ordinary tooling (cargo, rustc,
   node, npm, python3, go, bun, uv, git, ripgrep, clang, claude), **When** it runs under
   the narrowed profile, **Then** it exits with the same status and the same output as
   under the profile that granted the whole tree.
2. **Given** a program that asks for its own process information, or for that of a child
   it started inside the sandbox, **When** it runs under the narrowed profile, **Then**
   the read succeeds.

### Edge cases

- A tool needs a sysctl the allowlist does not name → the read is denied and the tool sees
  `EPERM`. The name is recoverable: the kernel logs `deny(1) sysctl-read <name>` for the
  sandboxed process, and extending the allowlist is a one-line change with a spec delta.
  Covered by FR-301 and named in the Assumptions.
- A tool inspects a process outside the sandbox on purpose (`ps`, `top`) → already
  impossible before this spec: both are setgid and the sandbox refuses to execute them.
  This spec closes the kernel interface they would have used.
- A process-information read whose target is a process group rather than a process
  (observed once, from `ruby`) → denied. It is not "inside the sandbox" in the sense
  FR-302 grants, and no measured tool changed behaviour because of it.
- `$HOME` cannot be resolved → the profile's home-relative denials cannot be written, but
  the denial FR-303 requires does not depend on `$HOME` and MUST still be emitted
  (FR-304).
- The target process is an Apple platform binary → the kernel already withholds its
  environment, whoever asks. That is not a property of this profile and this spec does not
  rely on it: the measured leak used a locally compiled holder, which is what a user's own
  tooling is.

## Requirements *(mandatory)*

### Functional

- **FR-301**: THE SYSTEM SHALL grant the sandboxed command read access to a fixed
  allowlist of sysctl names and name prefixes, and SHALL deny every sysctl read the
  allowlist does not name.
- **FR-302**: THE SYSTEM SHALL grant the sandboxed command process-information reads whose
  target is a process inside the same sandbox instance, and SHALL deny them for every
  process outside it.
- **FR-303**: THE SYSTEM SHALL deny, by an explicit denial rule, every
  process-information operation it does not grant.
- **FR-304**: IF `$HOME` cannot be resolved THEN THE SYSTEM SHALL still emit the denial
  required by FR-303.

### Non-functional

- **NFR-301**: A sandboxed command SHALL NOT be able to read the arguments or environment
  of any process outside its own sandbox instance, including the process of another
  concurrent sandme run.
- **NFR-302**: The allowlist required by FR-301 SHALL cover every sysctl name requested by
  the shipped recipes and by ordinary development tooling, such that all 40 commands in the
  measurement set exit with the same status under the narrowed profile as under the
  unrestricted one, and none of them changes its output except by reporting the narrowing
  itself.

## Constraints and dependencies

- macOS Seatbelt (SBPL via `sandbox-exec`), as required by SPEC-0001/NFR-001.
- The three parts of this change are jointly necessary, measured on Darwin 25.6.0: with any
  one of them dropped, the environment read succeeds. Narrowing `sysctl-read` alone does not
  close it, and neither does denying `process-info*` alone.
- `KERN_PROCARGS2` is reached through a numeric MIB carrying a pid, not through a name, so a
  rule naming `kern.procargs2` cannot express the denial. Measured: a
  `(deny sysctl-read (sysctl-name "kern.procargs2"))` rule — and a
  `(sysctl-name-prefix "kern.procargs")` one — changes nothing, whether placed before or
  after the grant it is meant to narrow.
- SBPL's last-match-wins rule, which SPEC-0002/NFR-101 relies on for path rules, does not
  hold for these operations: a blanket `(allow sysctl-read)` earlier in the profile defeats
  every later denial of this read. The grants have to be narrow where they are written.

## Success criteria *(mandatory)*

- **SC-001**: The probe from the issue, run under sandme against a process outside the
  sandbox, prints no environment variable of that process and reports `EPERM`.
- **SC-002**: All 40 commands in the measurement set — the two shipped recipes, eleven
  development tools and 27 system utilities — exit with the same status as before the
  change, and `cargo build` completes inside the sandbox.
- **SC-003**: A sandboxed command can still read its own process information and that of a
  process it started inside the sandbox.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-301 | `narrows_sysctl_reads_to_an_allowlist` (`src/profile.rs`) asserts the rule shape and the absence of a blanket grant; `denies_a_sysctl_the_allowlist_does_not_name` and `grants_the_sysctls_ordinary_programs_read` (`tests/cli.rs`) prove both halves against the kernel |
| FR-302 | `grants_process_information_only_inside_the_sandbox` (`src/profile.rs`); `denies_reading_another_process_environment` (`tests/cli.rs`) |
| FR-303 | `denies_every_ungranted_process_information_operation` (`src/profile.rs`) |
| FR-304 | `denies_process_information_without_a_home` (`src/profile.rs`) |
| NFR-301 | `denies_reading_another_process_environment` (`tests/cli.rs`). The process it reads from is a concurrent sandme run, so the cross-run clause is the same test. Its control run — the same probe, unsandboxed — must leak, or the assertion would pass for the wrong reason; both integration tests assert their control first, and both fail against the profile as it was |
| NFR-302 | Measurement procedure below, re-runnable; result recorded there with the date and OS build |

**NFR-302 measurement procedure.** Build the binary as it was before this change and the
binary after it, run each command in the set under both, and compare exit status and output.
Both sides run through `sandme` itself, not a hand-written profile, so what is measured is
what ships. The set is the Neovim recipe (`nvim --headless`; Zed is a GUI and was checked by
hand), twelve development tools (`cargo`, `rustc`, `node`, `npm`, `python3`, `go`, `bun`,
`uv`, `git`, `rg`, `fd`, `cc`, `claude`), and the system utilities a shell session reaches
for (`uname`, `getconf`, `uptime`, `sw_vers`, `hostname`, `df`, `arch`, `id`, `env`,
`openssl`, `perl`, `ruby`, `tar`, `curl`, `zsh`, `fish`, `sqlite3`, `xcrun`, `xargs`, `vim`,
`make`, `sysctl hw.ncpu`, `sysctl -a`, `vm_stat`, `ps`, `top`). The procedure MUST fail
rather than report when a command in the set is not installed: a path that does not exist
fails identically on both sides and reports as agreement.

Result, macOS 26 (Darwin 25.6.0, Apple silicon, arm64), 2026-09-09: **40 commands, 0
differences in exit status.** 37 were byte-identical. The three that differed:

| Command | Difference | Why |
| --- | --- | --- |
| `env` | proxy port and credential | Per invocation by design (SPEC-0003/FR-203) |
| `sysctl -a` | 1844 values before, 798 after | The narrowing, reported by the tool that reports it |
| `vm_stat` | page counters | Live machine state |

Sysctl names a run still asks for and does not get are read back from the kernel log:

```
log show --last 2m --predicate 'eventMessage CONTAINS "deny(1) sysctl-read"'
```

That is how the allowlist was derived, and how it should be extended.

## Assumptions

- **Requirement IDs use the 3xx block.** SPEC-0002 took 1xx and SPEC-0003 took 2xx;
  SPEC-0004 also took 2xx, so the per-spec block is no longer derivable from the spec
  number. 3xx is unused and this spec claims it whole.
- **The allowlist is what was measured, not what looked reasonable.** Every entry was
  requested by at least one command in the measurement set: the `hw.`, `machdep.cpu.`,
  `net.` and `sysctl.` prefixes, and `kern.argmax`, `kern.bootargs`, `kern.boottime`,
  `kern.hostname`,
  `kern.iossupportversion`, `kern.osproductversion`, `kern.osrelease`, `kern.ostype`,
  `kern.osvariant_status`, `kern.osversion`, `kern.version`,
  `security.mac.lockdown_mode_state` and `vm.loadavg`. Four names that seemed obviously
  needed — `kern.maxfilesperproc`, `kern.usrstack64`, `vm.overcommit`, `vm.swapusage` —
  were dropped when nothing asked for them.
- **`net.` is granted as a prefix.** Go's `net.Interfaces` reads the routing table through
  it and returns an error without it. This exposes the host's network topology, which
  `getifaddrs(3)` — which the profile does not mediate and this spec does not change —
  already exposes to the same command.
- **`sysctl.` is granted as a prefix.** It is the kernel's own metadata subtree, which every
  name-based read resolves through: without it `sysctl(8)` cannot report even a name this
  allowlist grants. It answers which knobs exist — public, and identical on every machine of
  the same build — and the values behind them still have to pass the allowlist, reached by
  name or by numeric MIB. Measured: a numeric-MIB read of a name outside the list is refused
  either way.
- **"Inside the same sandbox instance" is Seatbelt's `same-sandbox` target.** Measured
  against the two narrower alternatives: `self` also refuses a process its own child, and
  `children` refuses a grandchild, while `same-sandbox` covers the command's own process
  tree and stops at the sandbox boundary — including the boundary between two concurrent
  sandme runs.
- **The allowlist is expected to need extending.** It was measured on one machine, one OS
  build and one set of tools. A user hitting `EPERM` on a sysctl is a bug report with the
  name in it, and the fix is a spec delta plus a line.

## Open questions

None.

## Implementation tasks

- [x] **T-001** — Narrow `sysctl-read` to the measured allowlist, with the tests that
      assert its shape (covers FR-301)
- [x] **T-002** — Scope `process-info-pidinfo` to the sandbox instance and deny every
      other process-information operation explicitly, `$HOME` or no `$HOME`
      (covers FR-302, FR-303, FR-304)
- [x] **T-003** — Integration tests that prove the environment read is refused, with the
      unsandboxed control that gives the assertion meaning, and the cross-run case
      (covers NFR-301)
- [x] **T-004** — Run the NFR-302 measurement and record the result (covers NFR-302)

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-09 | Initial draft, from [#31](https://github.com/leopepe/sandme/issues/31). Status `Review`: the profile is observable behaviour and the change is presented for approval together with its implementation, as SPEC-0004 was. NFR-302 measured on macOS 26 (Darwin 25.6.0, Apple silicon, arm64): 40 of 40 commands unchanged in exit status, 37 byte-identical. Startup was measured too, though no budget requires it — SPEC-0001/NFR-003 is withdrawn: 11.7 ms median before, 11.6 ms after, 21 runs each, `sandme /usr/bin/true` — no change beyond noise. |
