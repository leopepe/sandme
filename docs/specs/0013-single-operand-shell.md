# SPEC-0013: Single-operand shell

## Metadata

- **Status**: Implemented
- **Created**: 2026-09-12
- **Updated**: 2026-09-12
- **Related ADRs**: none
- **Related specs**: SPEC-0001 (extends; the CLI surface and single-operand form)

## Summary

The single-operand form `sandme '<command>'` is routed through a shell so shell features work. This
spec fixes which shell: `/bin/bash`, not `/bin/sh`. On macOS `/bin/sh` is bash in POSIX mode, where
process substitution (`cat <(echo hi)`) is a syntax error — a common diff/compare pattern for an
IDE user, and one that surfaces as if the user's command were wrong rather than a shell sandme chose
([#36](https://github.com/leopepe/sandme/issues/36)).

## Spec deltas

### MODIFIED

- **SPEC-0001**, the single-operand execution form. Was: the single quoted operand is routed
  through `/bin/sh -c`. Now: routed through `/bin/bash -c`. Reason: `/bin/sh` (bash in POSIX mode)
  disables process substitution; `/bin/bash` enables it with fixed, machine-independent semantics.

## Problem

```text
$ sandme 'cat <(echo hi)'
/bin/sh: -c: line 0: syntax error near unexpected token `('
```

This is not the sandbox — macOS `/bin/sh` is bash in POSIX mode, and the same command fails outside
sandme. `/dev/fd` being granted (#14) was necessary but not sufficient: the shell still would not
parse the construct. The README leads with the single-operand form for anything needing shell
features, so a user reaching for `<(…)` — common in diff/compare workflows — got a syntax error.

## Goals

- The single-operand form supports process substitution and the other common bash features.
- The shell is fixed and machine-independent, so an invocation behaves the same everywhere.

## Non-goals

- Using `$SHELL`. It varies per machine, and fish/nushell `-c` semantics differ from POSIX, which
  would make the same invocation behave differently across machines and break sh-style one-liners.
- Changing the multi-operand form (`sandme ls -ltra ./`), which is a direct exec, not a shell.
- Bundling a newer bash. macOS ships bash 3.2; the features this spec relies on (process
  substitution, `&&`, globbing) are all present in 3.2.

## User scenarios *(mandatory)*

### Story 1 — Process substitution works in the quoted form (P1)

As someone running a shell one-liner under sandme, I want `<(…)` to work in `sandme '<command>'`, so
that diff/compare workflows do not fail with an opaque syntax error.

**Acceptance scenarios**

1. **Given** a single-operand command using process substitution, **When** it runs under sandme,
   **Then** the substitution is parsed and its output reaches the caller.

## Requirements *(mandatory)*

### Functional

- **FR-1301**: WHEN the user passes a single operand THE SYSTEM SHALL route it through `/bin/bash -c`.

### Non-functional

- **NFR-1301**: THE shell backing the single-operand form SHALL be a fixed absolute path, not
  derived from the environment, so the same invocation behaves identically on any macOS host.

## Interface contract

No new flags or config. The multi-operand form is unchanged. The only observable change is that
bash features (process substitution) now parse in the single-operand form.

## Success criteria *(mandatory)*

- **SC-1301**: `sandme 'cat <(echo hi)'` prints `hi` and exits `0`.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-1301, SC-1301 | `cli::runs_process_substitution_in_the_single_operand_form` (`tests/cli.rs`) |
| NFR-1301 | `src/sandbox.rs` `SHELL` is a `const &str` absolute path; no environment read backs it |

## Assumptions

- macOS bash 3.2 supports every feature the single-operand form advertises. Verified for process
  substitution on Darwin 25.6.0.

## Open questions

- None.

## Implementation tasks

- [x] **T-1301** — Route the single-operand form through `/bin/bash` via a named `SHELL` const
      (covers FR-1301, NFR-1301).
- [x] **T-1302** — Integration test for process substitution; update the README (covers SC-1301).

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-12 | Initial draft, with the implementation (single-operand form → `/bin/bash`). |
| 2026-09-12 | Shipped via #51; Status → Implemented (issue #34 reconciliation). |
