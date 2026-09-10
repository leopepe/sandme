# SPEC-0006: `sysctl-read` allowlist

## Metadata

- **Status**: Draft
- **Created**: 2026-09-10
- **Updated**: 2026-09-10
- **Related ADRs**: none yet
- **Related specs**: SPEC-0005 (deferred this from its scope), SPEC-0001 (would modify its
  baseline operation grants)

## Summary

The profile grants `sysctl-read` for the whole tree, so a sandboxed command sees the same 1844
values `sysctl -a` reports on the host — including `kern.proc.all`, the process table, which
names every process's pid, executable and uid. SPEC-0005 closed the environment read without
touching this. This spec proposes replacing the blanket grant with an allowlist of the names
ordinary programs actually read.

**Not implemented.** The measurement behind it is done; the decision to take on its maintenance
cost is not. See Open questions.

## Problem

SPEC-0005 denies a sandboxed command another process's *environment*. It leaves readable:

| Read | What it returns |
| --- | --- |
| `kern.proc.all` | the process table: pid, executable name, uid, ppid for every process |
| `kern.maxfilesperproc`, `kern.usrstack64`, `vm.swapusage` | host configuration and memory state |
| ~1800 further values | the rest of the tree |

None of it is the credential leak of issue #31, and none of it has a requirement behind it. It is
reconnaissance surface granted by a blanket rule that was never specified — the class issue #12
part 4 raises.

## Goals

- Grant `sysctl-read` only for names something in a measured set of tools actually reads.
- Deny the process table.
- Break no tool in that measured set.

## Non-goals

- The environment read of issue #31. SPEC-0005 closed it, and its rules stand whatever happens
  here.
- Escaping `shared_paths` before interpolation. An injected blanket `(allow sysctl-read)` outranks
  an allowlist, so this spec does not survive [#41](https://github.com/leopepe/sandme/issues/41)
  any more than SPEC-0005 does.
- `mach-lookup`, `mach-register`, `mach-bootstrap`, `lsopen`, `iokit-open` — the rest of issue #12
  part 4.

## User scenarios *(mandatory)*

### Story 1 — A sandboxed agent cannot enumerate what else is running (P1)

As someone running a coding agent I do not fully trust, I want the sandbox to hide the machine's
process table, so that the agent cannot see which tools and services I am running.

**Acceptance scenarios**

1. **Given** a sandboxed command, **When** it reads `kern.proc.all`, **Then** the kernel refuses.
2. **Given** a sandboxed command, **When** it reads a name the allowlist does not cover, **Then**
   the kernel refuses.
3. **Given** any tool in the measured set, **When** it runs under sandme, **Then** its exit status
   matches its exit status before the change.

### Edge cases

- **A tool needs a name the allowlist does not cover.** It gets `EPERM`, with no indication which
  name failed. The kernel log names it; FR-404 requires that route be documented.
- **`sysctl(8)` resolving a name.** It reads the kernel's own `sysctl.` metadata subtree before
  reading a value, so an allowlist without that subtree denies even a name it grants. This is what
  makes a name-filtered grant look unenforceable when tested through `sysctl(8)`; a C caller of
  `sysctlbyname` needs no metadata read and succeeds. Measured in PR #42.

## Requirements *(mandatory)*

### Functional

- **FR-401**: THE SYSTEM SHALL grant `sysctl-read` only for the name prefixes `hw.`,
  `machdep.cpu.`, `net.` and `sysctl.`, and the names `kern.argmax`, `kern.bootargs`,
  `kern.boottime`, `kern.hostname`, `kern.iossupportversion`, `kern.osproductversion`,
  `kern.osrelease`, `kern.ostype`, `kern.osvariant_status`, `kern.osversion`, `kern.version`,
  `security.mac.lockdown_mode_state` and `vm.loadavg`.
- **FR-402**: THE SYSTEM SHALL deny `sysctl-read` of `kern.proc` and every name beneath it.
- **FR-403**: THE SYSTEM SHALL retain SPEC-0005/FR-301, so the `kern.procargs` denial stands at a
  specificity a blanket grant injected through `shared_paths` cannot outrank.
- **FR-404**: THE SYSTEM SHALL document, in `README.md`, the kernel-log query that names a sysctl
  the allowlist denied, so a user hitting `EPERM` can report which name their tool needs.

### Non-functional

- **NFR-401**: The change SHALL alter the exit status of no tool in the measured set of 40
  commands, and SHALL leave at least 37 of their outputs byte-identical.
- **NFR-402**: The change SHALL add no more than 1 ms to median startup over 21 runs.

## Interface contract

No change to the CLI surface or the configuration keys. The allowlist is not configurable: a
`shared_paths`-style knob for kernel reads would be a second injection surface, and no
requirement asks for one.

## Constraints and dependencies

- The `sysctl.` metadata subtree is load bearing for `sysctl(8)` and anything else resolving a
  name through the kernel rather than a numeric MIB.
- `kern.boottime` is load bearing beyond `uptime(1)`: Node's `os.uptime()` aborts without it.
- `net.` carries the routing table; without it Go's `net.Interfaces` returns an error. The
  topology it exposes is already available unmediated through `getifaddrs(3)`.

## Success criteria *(mandatory)*

- **SC-401**: `kern.proc.all` is refused inside the sandbox.
- **SC-402**: `sysctl -a` reports about 798 values inside the sandbox against 1844 outside.
- **SC-403**: Every tool in the measured set exits as it did before the change.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-401 | a unit test asserting every prefix and name is granted, and no other |
| FR-402 | an integration test reading `kern.proc.all` inside the sandbox |
| FR-403 | SPEC-0005's `cli::denies_reading_another_process_environment_under_an_injected_grant` |
| FR-404 | review of `README.md` |
| NFR-401 | the 40-command differential run, exit status and output compared |
| NFR-402 | 21 runs before and after, median compared |

Measurement already carried out in PR #42 on macOS 26 (Darwin 25.6.0, Apple silicon): 40 commands
including `nvim --headless`, `cargo build`, `rustc`, `node`, `npm`, `python3`, `go build`, `bun`,
`uv`, `git`, `rg`, `fd`, `cc`, `claude` and 26 system utilities. **0 differences in exit status,
37 outputs byte-identical.** The three that differed: `env` (per-run proxy credential), `sysctl -a`
(1844 values → 798, the narrowing reported by the tool that reports it) and `vm_stat` (live
counters). Startup 11.7 ms → 11.6 ms median over 21 runs.

## Assumptions

- The measured set represents what users sandbox. A tool outside it reading an unlisted name
  breaks, and the allowlist grows by one entry per report.

## Open questions

- [ ] Is the ongoing maintenance worth the narrowing? Every future tool reading an unlisted name
      fails with `EPERM` and needs a release to fix. Blocks FR-401.
- [ ] Should `kern.proc` be denied on its own (FR-402) without the full allowlist? That closes the
      process table — the concrete exposure this spec names — at a fraction of the cost, and the
      remaining ~1800 values are configuration rather than reconnaissance. Blocks FR-401.

## Implementation tasks

- [ ] **T-001** — Resolve the open questions above. Nothing below starts until they are answered.
- [ ] **T-002** — Replace the blanket grant with the allowlist (covers FR-401, FR-402, FR-403).
- [ ] **T-003** — Unit and integration tests per the Verification table.
- [ ] **T-004** — Document the allowlist and the kernel-log query in `README.md` (covers FR-404).
- [ ] **T-005** — Re-run the 40-command differential and the startup measurement on the merged
      branch (covers NFR-401, NFR-402).
- [ ] **T-006** — `/security-audit`, required by `AGENTS.md` for any change to `src/profile.rs`.

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-10 | Initial draft. Deferred out of SPEC-0005; measurement adopted from PR #42. |
