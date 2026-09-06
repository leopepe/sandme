# SPEC-0004: Exit status reservations

## Metadata

- **Status**: Review
- **Created**: 2026-09-06
- **Updated**: 2026-09-06
- **Related ADRs**: none
- **Related specs**: SPEC-0001 (supersedes its **Exit codes / errors** contract)

## Summary

sandme returns `1` for every failure of its own — unreadable or malformed config, a proxy
that cannot bind, a command it cannot start — and `1` is the most common status a wrapped
command returns. A caller reading the status cannot tell the two apart. This spec reserves
`125` for sandme's own failures, and makes sandme report `127` for a command that does not
exist and `126` for one that exists but cannot be executed, in place of the `71` that
`sandbox-exec` currently leaks through the wrapper.

## Spec deltas

### MODIFIED

- **SPEC-0001/Interface contract — Exit codes / errors** — was: `1`–`125` is "the sandboxed
  command's own exit code, propagated unchanged — including statuses produced by the sandbox
  layer itself, e.g. `71` when the command is not found. `1` is also sandme's own failure
  code". Now: `1`–`124` is the wrapped command's own status; `125` is reserved for sandme's
  own failures; `126` and `127` report an exec failure sandme has identified; `128+n` is
  unchanged. Reason: the old row both violates
  `docs/guidelines/architecture/posix.md` §4 (sandme's own failures collide with the
  wrapped command's `1`) and contradicts the same section's `126`/`127` rows
  ([#11](https://github.com/leopepe/sandme/issues/11)).

### ADDED

- FR-201 … FR-205 below.

Nothing is REMOVED: FR-001 … FR-009 of SPEC-0001 are untouched. The **Exit codes / errors**
table in SPEC-0001 is an interface contract rather than a numbered requirement, so it carries
no ID to withdraw; this spec's table replaces it and is the current contract.

## Problem

`src/main.rs` returns `ExitCode::FAILURE` (`1`) on all three of its own failure paths, and
`1` is what `wc` returns for a missing file, `grep` for no match and `diff` for a difference.
The consumer that motivated [#11](https://github.com/leopepe/sandme/issues/11) records the
status of every invocation it makes as data; a sandme failure recorded as `1` becomes an
entry claiming the command under test failed.

`docs/guidelines/architecture/posix.md` §4 already forbids the collision — "`sandme`'s own
failures MUST NOT collide with statuses the wrapped command can plausibly return where the
spec can avoid it; reserve and document them" — so this is a conformance gap against a rule
the project wrote down, not a new capability.

A second gap sits next to it. The same section assigns `127` to "the command was not found"
and `126` to "found but could not be executed"; sandme produces neither. Measured on macOS 26
(Darwin 25.6.0) at `65a2c42`:

| Invocation | Status today |
| --- | --- |
| `sandme no-such-command-xyz arg` | `71`, with `sandbox-exec: execvp() of 'no-such-command-xyz' failed: No such file or directory` on the child's stderr |
| `sandme ./not-executable.sh arg` | `71`, with `… failed: Permission denied` on the child's stderr |
| `sandme 'no-such-command-xyz'` | `127` (the single operand goes to `/bin/sh -c`, and the shell already follows the convention) |
| `sandme './not-executable.sh'` | `126` (likewise) |

So the direct-exec path and the shell path disagree with each other, and the direct-exec path
disagrees with posix.md. `71` is `EX_OSERR` from `sandbox-exec`'s own `sysexits.h` vocabulary;
it means nothing to a caller that knows the exec-wrapper convention, and `sandbox-exec` uses
the one value for every `execvp` failure, distinguishing the causes only in prose on stderr.

## Goals

- Reserve one status for sandme's own failures, distinct from anything a wrapped command
  plausibly returns, and document it.
- Report the two exec failures with the statuses posix.md §4 already assigns to them,
  identically whether the command was given as operands or inside a quoted shell string.
- Keep the wrapped command's own status, and `128+n` for signal deaths, exactly as they are.

## Non-goals

- Suppressing or rewriting `sandbox-exec`'s own stderr. The child's streams pass through
  unmodified (posix.md §3), so its `execvp()` message stays where it is; sandme adds its own
  `sandme: `-prefixed line beside it. The wider stderr-attribution question raised in #11
  stays open.
- Making sandme's `125` distinguishable from a wrapped command that itself exits `125`. One
  integer cannot carry two facts; see **Assumptions**.
- Reclassifying any other status `sandbox-exec` may return, or inspecting the child's stderr
  to do so.
- A `--exit-status`-style flag, or any option that lets the caller remap these values.

## User scenarios *(mandatory)*

### Story 1 — Tell a sandme failure from a command failure (P1)

As a developer scripting around sandme, I want sandme's own failures to carry a status no
ordinary command returns, so that my script can tell "sandme could not run this" from "this
ran and failed".

**Acceptance scenarios**

1. **Given** `~/.sandme/config.toml` contains invalid TOML, **When** the user runs
   `sandme echo hi`, **Then** sandme exits `125` and writes a `sandme: `-prefixed diagnostic
   naming the cause to stderr.
2. **Given** the configured proxy port is already in use, **When** the user runs
   `sandme echo hi`, **Then** sandme exits `125` and says on stderr that the port could not
   be taken.
3. **Given** a command that exits `3`, **When** it is run under sandme, **Then** sandme exits
   `3`.
4. **Given** a command that kills itself with `SIGINT`, **When** it is run under sandme,
   **Then** sandme exits `130`.

### Story 2 — Recognise a command that never ran (P2)

As a developer cataloguing a CLI's behaviour through sandme, I want a command that does not
exist to report the status every shell reports for it, so that my results do not depend on
which sandbox implementation sandme happens to use.

**Acceptance scenarios**

1. **Given** a name nothing on `PATH` answers to, **When** the user runs
   `sandme no-such-command-xyz arg`, **Then** sandme exits `127` and writes
   `sandme: no-such-command-xyz: command not found` to stderr.
2. **Given** an existing file with no execute permission, **When** the user runs
   `sandme ./not-executable.sh arg`, **Then** sandme exits `126` and writes
   `sandme: ./not-executable.sh: found but not executable` to stderr.
3. **Given** the same two commands inside a single quoted operand, **When** they are run,
   **Then** sandme exits `127` and `126` respectively — the statuses `/bin/sh` already
   produces.

### Edge cases

- The wrapped command itself exits `125`, `126` or `127` → propagated unchanged. sandme
  never clamps or rewrites a status a command produced by running; the collision is
  documented under **Assumptions** rather than engineered away.
- `sandbox-exec` exits `71` for a reason that is not a missing or non-executable command —
  a profile it will not compile, or a child that chose `71` for itself — → propagated
  unchanged. sandme reinterprets `71` only when it can independently establish that the
  program it named cannot be executed.
- The program is executable but `execvp` still fails (an unknown interpreter after `#!`, a
  binary for another architecture) → `71` is propagated unchanged; sandme's check answers
  "this could be executed" and the truth is more specific than it can see.
- `sandbox-exec` itself cannot be started (it is missing, or `fork` fails) → `125`. That is
  sandme's own failure, not the wrapped command's.
- The command is named inside a quoted shell string → `/bin/sh` resolves it, and its `126`
  and `127` already match this contract. sandme leaves the shell's status alone.

## Requirements *(mandatory)*

### Functional

- **FR-201**: IF sandme cannot read or parse its configuration file, cannot bind the proxy
  port, or cannot start `sandbox-exec` THEN THE SYSTEM SHALL exit `125`.
- **FR-202**: WHEN THE SYSTEM exits `125` it SHALL write a diagnostic naming the cause to
  stderr, prefixed `sandme: `.
- **FR-203**: IF the sandboxed command could not be executed because nothing of that name
  was found THEN THE SYSTEM SHALL exit `127`.
- **FR-204**: IF the sandboxed command could not be executed because the file that was found
  is not executable THEN THE SYSTEM SHALL exit `126`.
- **FR-205**: WHEN the sandboxed command runs, THE SYSTEM SHALL propagate its exit status
  unchanged, including the values `125`, `126` and `127`, and SHALL report termination by
  signal `n` as `128+n`.

### Non-functional

- **NFR-201**: THE SYSTEM SHALL determine FR-203 and FR-204 from the filesystem alone, and
  SHALL NOT parse the child's stderr to do so — the child's streams belong to the child
  (posix.md §3).

## Interface contract

**Exit codes / errors** *(supersedes the table in SPEC-0001)*

| Code | Condition | Message to user |
| --- | --- | --- |
| `0` | The sandboxed command exited `0`. | — |
| `1`–`124` | The sandboxed command's own exit status, propagated unchanged. | — |
| `125` | sandme's own failure: config unreadable or malformed, the proxy could not bind its port, or `sandbox-exec` could not be started. | `sandme: ` followed by the cause, on stderr. |
| `126` | The command was found but is not executable. | `sandme: <command>: found but not executable` |
| `127` | The command was not found. | `sandme: <command>: command not found` |
| `128+n` | The sandboxed command was terminated by signal `n` (Ctrl-C gives `130`). | — |
| any other | The sandboxed command's own exit status, propagated unchanged — including `125`, `126`, `127` when the command itself chose them, and `71` when `sandbox-exec` failed for a reason sandme could not identify. | — |

`125`, `126` and `127` are the values `env(1)` and `timeout(1)` reserve for the same three
conditions; a caller that already knows the exec-wrapper convention needs to learn nothing
new.

## Key entities

- **sandme's own failure**: a failure that happened before, or instead of, the wrapped
  command running to completion. It always carries a `sandme: `-prefixed diagnostic.
- **Exec failure**: `sandbox-exec` reached `execvp` and it failed. Reported by
  `sandbox-exec` as `71` for every cause; sandme distinguishes the two causes posix.md names
  by resolving the program itself.

## Constraints and dependencies

- `sandbox-exec` (macOS Seatbelt) collapses every `execvp` failure to status `71`. Verified
  on macOS 26 (Darwin 25.6.0): both a missing command and a non-executable file exit `71`,
  differing only in the `errno` text on stderr. The distinction posix.md §4 asks for is
  therefore not readable from the status, and sandme must reconstruct it.
- A process's exit status is one byte. Everything below follows from that: no status can be
  both reserved and free.
- No new dependency: the resolution `execvp` performs (a literal path, or a search of `PATH`)
  is a handful of `std::fs` calls, and `src/app_bundle.rs` already performs it to find app
  bundles.

## Success criteria *(mandatory)*

- **SC-201**: For each of the three sandme failure paths, the observed status is `125` and
  stderr carries a `sandme: ` line naming the cause.
- **SC-202**: `sandme no-such-command-xyz arg` and `sandme 'no-such-command-xyz'` both exit
  `127`; `sandme ./not-executable.sh arg` and `sandme './not-executable.sh'` both exit `126`.
- **SC-203**: `sandme sh -c 'exit 3'` exits `3`, `sandme sh -c 'kill -INT $$'` exits `130`,
  and `sandme sh -c 'exit 125'` exits `125` — no status a command produced is rewritten.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-201 | `reports_a_malformed_config_with_the_reserved_status`, `reports_a_proxy_that_cannot_bind_with_the_reserved_status` (`tests/cli.rs`); `sandbox-exec` missing is not reproducible on a healthy host and is covered by the shared `failure_code` mapping the first two exercise. |
| FR-202 | The same two tests assert the `sandme: ` prefix and the cause. |
| FR-203 | `reports_a_command_it_cannot_find_as_127`, `reports_a_missing_command_in_a_shell_string_as_127` (`tests/cli.rs`); `reports_a_name_nothing_answers_to` (unit, `src/executable.rs`). |
| FR-204 | `reports_a_command_that_is_not_executable_as_126`, `reports_a_non_executable_command_in_a_shell_string_as_126` (`tests/cli.rs`); `reports_a_path_that_is_not_executable` (unit, `src/executable.rs`). |
| FR-205 | `passes_multi_word_arguments_unchanged` (exit `3`), `reports_signal_deaths_as_128_plus_n` (`130`), `propagates_a_childs_own_reserved_status` (`125`), `propagates_an_unexplained_exec_status` (`71`) (`tests/cli.rs`). |
| NFR-201 | `reports_no_failure_for_an_executable` (unit, `src/executable.rs`) — the classification takes only a command word and reads only the filesystem; the child's streams are never captured. |

## Assumptions

- **A wrapped command that itself exits `125` is indistinguishable from a sandme failure by
  status alone, and this is accepted.** The status is one byte and both facts want the same
  byte; `env(1)` and `timeout(1)` accept the same collision. The residual risk is small — a
  command choosing `125` is rare, where a command choosing `1` is routine — and the caller
  who must be certain has a reliable second channel: sandme's `125` always comes with a
  `sandme: `-prefixed line on stderr, and sandme never prefixes anything it did not write.
  The same holds for `126` and `127`.
- Reinterpreting `71` only when sandme can independently establish the cause is preferred
  over mapping every `71`. A command that ran and chose `71` for itself resolves as
  executable and keeps its status, so the reinterpretation cannot swallow it.
- sandme's `PATH` is the child's `PATH`: the child inherits sandme's environment, so
  resolving the program in sandme's process reaches the same file `sandbox-exec` would have.

## Open questions

None.

## Implementation tasks

- [x] **T-201** — Reserve `125`: map every `SandmeError` that is sandme's own failure to it,
  with the diagnostic unchanged (covers FR-201, FR-202)
- [x] **T-202** — Resolve the named program the way `execvp` does, and turn `sandbox-exec`'s
  `71` into `127` or `126` when that resolution explains it (covers FR-203, FR-204, NFR-201)
- [x] **T-203** — Tests: integration tests for every row of the exit-code table, unit tests
  for the resolution (covers FR-201 … FR-205, NFR-201)
- [x] **T-204** — Correct the `1`–`125` row of `docs/guidelines/architecture/posix.md` §4,
  which contradicted the rule stated three lines below it, and the README's exit-status
  paragraph (covers FR-201, FR-205)

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-06 | Initial draft, from [#11](https://github.com/leopepe/sandme/issues/11). Status `Review`: the exit codes are observable behaviour and the change is presented for approval together with its implementation. |
