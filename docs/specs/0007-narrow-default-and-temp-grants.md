# SPEC-0007: Narrow the default share, the GUI temp grant, and AppleEvents

## Metadata

- **Status**: Draft
- **Created**: 2026-09-10
- **Updated**: 2026-09-11
- **Related ADRs**: none yet
- **Related specs**: SPEC-0001 (extends; modifies the `shared_paths` default), SPEC-0002
  (extends; narrows the `gui_mode` temp grant it introduced)

## Summary

sandme's sandbox still grants more than the user asked for in three ways reported in
[#12](https://github.com/leopepe/sandme/issues/12): the default `shared_paths = ["~/"]` shares
the whole home directory out of the box, `gui_mode` grants read-write to *all* of `/private/tmp`
and `/private/var/folders` rather than the invoking user's own temporary directory, and the
`appleevent-send` operation is left permitted. This spec narrows the default share to the current
working directory, scopes the `gui_mode` temp grant to the per-user temporary directory
(`$TMPDIR`), and denies `appleevent-send` unconditionally.

The fourth widening in #12 — the blanket `(allow mach-lookup)` with no service allowlist — is
left open (see Open questions): a safe minimal allowlist could not be established with evidence,
and over-narrowing it breaks DNS and ordinary tooling.

## Spec deltas

### MODIFIED

- **SPEC-0001/`shared_paths` default** — was: `["~/"]` (the whole home directory). Now: the
  current working directory at the time sandme is invoked. Reason: `["~/"]` exposes `~/.ssh`,
  `~/.aws`, `~/.gnupg` and shell history to any sandboxed command out of the box (#12 §2); a
  user working on one project rarely needs more than the directory they are standing in, and
  anyone who wants the old behaviour sets `shared_paths = ["~/"]` explicitly.
- **SPEC-0002/FR-101** — was: WHILE `gui_mode` is enabled THE SYSTEM SHALL grant the sandboxed
  command read and write access to `~/Library`, `/private/tmp` and `/private/var/folders`. Now:
  WHILE `gui_mode` is enabled THE SYSTEM SHALL grant the sandboxed command read and write access
  to `~/Library` and to the per-user temporary directory named by `$TMPDIR`, and SHALL NOT grant
  `/private/tmp` or the whole of `/private/var/folders`. Reason: `/private/tmp` is world-shared
  and `/private/var/folders` holds every user account's per-app containers; a GUI application
  needs only its own scratch space, which lives under `$TMPDIR` (#12 §3). FR-102, FR-103 and
  FR-104 of SPEC-0002 are unchanged.

## Problem

Reproduced against the built binary on macOS 26 (Darwin 25.6.0, Apple silicon):

- **Default share.** With no `shared_paths` configured, a sandboxed command writes
  `~/.ssh/evil` successfully — the whole home directory is shared. Confinement is opt-in when it
  should be the default posture.
- **GUI temp grant.** With `gui_mode` on, a sandboxed command writes `/private/tmp/<anything>`
  and `/private/var/folders/<other account or app>/…` successfully. Both are shared surfaces: a
  channel to unsandboxed processes and to other applications' containers, granted for scratch
  space the application keeps under its own `$TMPDIR`.
- **AppleEvents.** `osascript -e 'tell application "Finder" to get name'` returns `Finder` from
  inside the sandbox, and the `appleevent-send` operation is permitted, so the cross-application
  scripting channel is not explicitly closed.

## Goals

- Make the out-of-the-box default share the current working directory, not the home directory.
- Scope the `gui_mode` temporary-directory grant to the per-user `$TMPDIR`, keeping scratch
  space (editors, `diff` of process substitutions, git's `xcrun` shim per
  [#29](https://github.com/leopepe/sandme/issues/29)) working.
- Deny the `appleevent-send` operation in every configuration.

## Non-goals

- Narrowing the blanket `(allow mach-lookup)` / `mach-register` / `mach-bootstrap` / `lsopen` to
  a service allowlist (#12 §4). Deferred — see Open questions.
- A read-only share ([#10](https://github.com/leopepe/sandme/issues/10)).
- Granting the per-user *cache* directory (`_CS_DARWIN_USER_CACHE_DIR`) or any `~/Library`
  subpath beyond what SPEC-0002 already governs.
- Closing the `osascript … get name` read: it returns a static application name resolved locally
  without an AppleEvent round-trip, is not an escape vector, and closing it would cost ordinary
  scripting without a security gain.

## User scenarios *(mandatory)*

### Story 1 — A fresh install confines to the project (P1)

As someone who runs `sandme <tool>` in a project directory without configuring anything, I want
the tool to see that directory and not my whole home, so that `~/.ssh` and `~/.aws` are not
exposed by default.

**Acceptance scenarios**

1. **Given** no `shared_paths` configured and the shell sitting in a project directory, **When**
   the sandboxed command writes a file in that directory, **Then** the write succeeds.
2. **Given** the same, **When** the sandboxed command writes `~/.ssh/evil` (outside the working
   directory), **Then** the write is refused.

### Story 2 — GUI scratch space works, shared temp does not (P1)

As someone launching an editor under `gui_mode`, I want its own scratch space to work while the
world-shared temp directories stay closed, so that GUI mode does not re-open a channel to
unsandboxed processes.

**Acceptance scenarios**

1. **Given** `gui_mode` on, **When** the command writes a file under `$TMPDIR`, **Then** the
   write succeeds.
2. **Given** `gui_mode` on, **When** the command writes a file directly under `/private/tmp`,
   **Then** the write is refused.

### Edge cases

- **`$TMPDIR` is unset.** No per-user temp rule can be built; `gui_mode` grants `~/Library` only
  and the sandbox is otherwise unaffected. Covered by FR-702 ("named by `$TMPDIR`" — absent when
  `$TMPDIR` is).
- **The working directory is a symlink or reached through `/var`.** The grant is built from the
  resolved path, as every `shared_paths` entry already is (SPEC-0002 FR-103 applies unchanged).
- **A genuine AppleEvent round-trip.** Denied by FR-703; on this host it already fails with
  `-600` for independent reasons, so the denial is defense-in-depth (see Assumptions).

## Requirements *(mandatory)*

### Functional

- **FR-701**: WHEN no `shared_paths` is configured by file or environment THE SYSTEM SHALL
  default `shared_paths` to the current working directory at invocation time.
- **FR-702**: WHILE `gui_mode` is enabled THE SYSTEM SHALL grant read and write access to the
  per-user temporary directory named by `$TMPDIR`, and SHALL NOT grant read or write access to
  `/private/tmp` or to `/private/var/folders` as a whole.
- **FR-703**: THE SYSTEM SHALL deny the sandboxed command the `appleevent-send` operation,
  whatever `shared_paths` and `gui_mode` are set to.
- **FR-704**: IF a path bound for a profile rule — a `shared_paths` entry, `$HOME`, `$TMPDIR`, or
  the working directory — contains a double quote, a backslash, or an ASCII control character,
  THEN THE SYSTEM SHALL fail the invocation before running the command, rather than emit the
  profile. Those characters can close a `(subpath "…")` literal and inject arbitrary SBPL;
  parentheses, spaces and other characters are inert inside the literal and are permitted.
  Introduced because FR-701's working-directory default and FR-702's `$TMPDIR` grant add two new
  environment-fed values to the profile; it also closes the pre-existing `shared_paths` sink
  ([#41](https://github.com/leopepe/sandme/issues/41)).

### Non-functional

- **NFR-701**: The `gui_mode` temp grant SHALL keep the per-user temporary directory writable, so
  that a tool placing scratch files under `$TMPDIR` — an editor, macOS `diff` on a process
  substitution, git's `xcrun` shim (#29) — continues to work after the grant is narrowed.

## Interface contract

**Configuration**

| Key | Env var | Type | Default | Description |
| --- | --- | --- | --- | --- |
| `shared_paths` | `SANDME_SHARED_PATHS` | array of strings (env: comma-separated) | the current working directory | Paths the sandboxed command may read and write. Set `["~/"]` to restore the whole-home default. |
| `gui_mode` | `SANDME_GUI_MODE` | boolean (env: `1` or `true`) | `false` | Also grants read+write to `~/Library` and to `$TMPDIR`. No longer grants `/private/tmp` or all of `/private/var/folders`. |

No new key, flag or exit code.

## Constraints and dependencies

- macOS Seatbelt (`sandbox-exec -p`), per SPEC-0001 NFR-001. `appleevent-send` and `$TMPDIR`
  subpath grants are ordinary SBPL; `sandbox-exec` rejects an unknown operation name, so
  `appleevent-send` being accepted confirms it is a real operation.
- The per-user temporary directory is read from `$TMPDIR`, which macOS sets per login session and
  which the sandboxed child inherits — so the directory sandme grants is exactly the one the
  child will use for temp files.

## Success criteria *(mandatory)*

- **SC-701**: With nothing configured, a sandboxed process can write in the working directory and
  cannot write `~/.ssh`, verified by running the command and checking the refusal.
- **SC-702**: With `gui_mode = 1`, a sandboxed process can write under `$TMPDIR` and cannot write
  directly under `/private/tmp`, verified by running the command.
- **SC-703**: The generated profile denies `appleevent-send`.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-701 | `default_share_is_the_current_directory` (`src/config.rs`); `confines_to_the_working_directory_by_default` and `denies_the_home_outside_the_working_directory_by_default` (`tests/cli.rs`) |
| FR-702 | `gui_mode_grants_the_per_user_temp_dir_only` (`src/profile.rs`); `denies_the_world_temp_directory_under_gui_mode` (`tests/cli.rs`) |
| FR-703 | `denies_the_appleevent_send_operation` (`src/profile.rs`) |
| FR-704 | `refuses_a_shared_path_that_would_break_out_of_the_profile` and `admits_a_shared_path_with_parentheses_and_spaces` (`src/profile.rs`); `refuses_to_run_when_tmpdir_would_inject_into_the_profile` (`tests/cli.rs`) — the last is the `/security-audit` PoC, run end-to-end against the binary |
| NFR-701 | `allows_a_git_style_temp_write_under_gui_mode` (`tests/cli.rs`) |

## Assumptions

- A user who runs `sandme <tool>` without configuring `shared_paths` is working in the directory
  they invoked it from, so the working directory is the useful default. The whole-home default
  remains one explicit `shared_paths = ["~/"]` away, and the README's recipes already set
  `shared_paths` explicitly.
- `$TMPDIR` names the per-user temporary directory (`_CS_DARWIN_USER_TEMP_DIR`) and is the
  directory the child itself uses for temp files. Reading it avoids an FFI `confstr` call and a
  new dependency, and grants precisely where the child writes.
- Denying `appleevent-send` is correct defense-in-depth even though its effect is not observable
  on the test host: the demonstrated `osascript … get name` returns a locally-resolved constant
  (no round-trip), and a genuine round-trip already fails with `-600` whether or not the
  operation is denied. The denial removes a permission the sandbox has no reason to hold; it is
  not relied on to close an otherwise-open escape.

## Open questions

None blocking. Recorded for a later spec:

- Narrowing the blanket `mach-lookup` family (#12 §4) to a service allowlist is deferred. A
  minimal safe set could not be established with evidence on the test host, and over-narrowing
  breaks DNS (`mDNSResponder`) and ordinary tooling. This is scoped out under Non-goals rather
  than guessed at.

## Implementation tasks

- [x] **T-701** — Default `shared_paths` to the current working directory; update the config
      unit tests and the README default (covers FR-701)
- [x] **T-702** — Scope the `gui_mode` temp grant to `$TMPDIR`, dropping `/private/tmp` and
      `/private/var/folders`; update the profile unit test and the README (covers FR-702, NFR-701)
- [x] **T-703** — Emit `(deny appleevent-send)` in the profile (covers FR-703)
- [x] **T-704** — Integration tests driving the binary for each new denial and each retained
      grant (covers FR-701, FR-702, NFR-701)
- [x] **T-705** — Reject profile-bound paths carrying SBPL-breaking characters, fail-shut, with a
      new `SandmeError` variant; unit tests plus the `/security-audit` PoC end-to-end (covers
      FR-704)

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-10 | Initial draft: issue #12 §2, §3 and the `appleevent-send` half of §4. The mach-lookup allowlist (§4) is deferred as an open question. |
| 2026-09-11 | Add FR-704: reject profile-bound paths carrying SBPL-breaking characters (fail-shut), closing the injection the `/security-audit` found in the new `$TMPDIR` grant and the pre-existing `shared_paths` sink (#41). |
