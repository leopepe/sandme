# SPEC-0002: Filesystem grants outside `shared_paths`

## Metadata

- **Status**: Review
- **Created**: 2026-09-06
- **Updated**: 2026-09-06
- **Related ADRs**: none yet
- **Related specs**: SPEC-0001 (extends; adds requirements SPEC-0001 never stated)

## Summary

sandme's Seatbelt profile grants filesystem access that `shared_paths` does not describe:
`~/Library` unconditionally, and the temporary directories when `gui_mode` is set. Neither
grant appears in SPEC-0001 — its configuration table lists only `shared_paths` and
`proxy_port`. This spec states what those grants are, ties them to `gui_mode`, and adds a
small set of directories that no configuration may reach.

## Problem

The profile grants substantially more than the user configured, and the specification says
nothing about it, so neither behaviour can be reviewed against a contract.

Two concrete consequences, reported in
[#12](https://github.com/leopepe/sandme/issues/12) and reproduced on macOS 26 (Darwin
25.6.0, Apple silicon) with `shared_paths` narrowed to a single scratch directory:

- `~/Library/Keychains/login.keychain-db` is readable.
- `~/Library/LaunchAgents/*.plist` is **writable**. A plist written there is executed by
  launchd at the next login, outside the sandbox. That is a persistence escape available to
  any sandboxed process, however narrowly the user configured `shared_paths`.

The premise behind the grant is sound — GUI applications do keep state, caches and
preferences under `~/Library` — but a command that is not a GUI application needs none of
it, and no application needs the two launchd directories or the keychain store.

## Goals

- State the filesystem grants sandme makes beyond `shared_paths`, and what turns them on.
- Make `~/Library` access a consequence of asking for GUI mode, not a constant.
- Put launchd's job directories and the keychain store out of reach of every configuration,
  including the default `shared_paths = ["~/"]`.

## Non-goals

These are real problems recorded in #12 and #10. They are deliberately left to later specs
so this one stays reviewable in one sitting:

- Narrowing the default `shared_paths = ["~/"]` (#12 §2).
- Scoping the `gui_mode` temp grant to the launched application's own container instead of
  all of `/private/tmp` and `/private/var/folders` (#12 §3).
- Replacing the blanket `(allow mach-lookup)` with a service allowlist (#12 §4).
- A read-only share (#10).
- Per-bundle `~/Library` subpaths. GUI mode grants `~/Library` whole, minus the denials in
  FR-102.

## User scenarios *(mandatory)*

### Story 1 — A narrow share stays narrow (P1)

As someone sandboxing a tool I do not fully trust, I want the sandbox to grant what I
configured and not a home directory's worth more, so that reading `shared_paths` tells me
what the tool can reach.

**Acceptance scenarios**

1. **Given** `shared_paths` naming one scratch directory and `gui_mode` off, **When** the
   sandboxed command writes to `~/Library/Application Support`, **Then** the write is
   refused.
2. **Given** the same configuration, **When** the sandboxed command reads
   `~/Library/Preferences`, **Then** the read is refused.

### Story 2 — An editor still works (P1)

As someone launching an editor or IDE under sandme, I want its state, caches and preferences
to survive, so that GUI mode is enough to make it usable.

**Acceptance scenarios**

1. **Given** `gui_mode` on, **When** the command writes under `~/Library/Application
   Support`, **Then** the write succeeds.

### Story 3 — No persistence escape (P1)

As someone relying on the sandbox to contain a process, I want it to be unable to arrange
for code to run outside the sandbox later, so that the confinement does not end at the
process's lifetime.

**Acceptance scenarios**

1. **Given** the default `shared_paths = ["~/"]` and `gui_mode` on — the widest configuration
   sandme offers — **When** the sandboxed command writes
   `~/Library/LaunchAgents/probe.plist`, **Then** the write is refused.
2. **Given** the same configuration, **When** the sandboxed command reads
   `~/Library/Keychains/login.keychain-db`, **Then** the read is refused.

### Edge cases

- **`$HOME` reaches its target through a symlink.** Seatbelt matches the path the kernel
  resolves, so a rule built from the raw `$HOME` would not match and would silently grant or
  deny nothing. Covered by FR-103.
- **`$HOME` is unset.** No home-relative rule can be built; the profile is emitted without
  them and the rest of the sandbox is unaffected.
- **A user lists `~/Library/LaunchAgents` in `shared_paths` explicitly.** FR-102 still wins;
  the denial is not configurable.
- **An application uses Keychain Services.** `securityd` reads the keychain files on the
  process's behalf, so FR-102 does not break credential storage.

## Requirements *(mandatory)*

### Functional

- **FR-101**: WHILE `gui_mode` is enabled THE SYSTEM SHALL grant the sandboxed command read
  and write access to `~/Library`, `/private/tmp` and `/private/var/folders`.
- **FR-102**: THE SYSTEM SHALL deny the sandboxed command read and write access to
  `~/Library/Keychains`, `~/Library/LaunchAgents` and `~/Library/LaunchDaemons`, whatever
  `shared_paths` and `gui_mode` are set to.
- **FR-103**: THE SYSTEM SHALL build every home-relative rule from `$HOME` with its symlinks
  resolved, so the rule names the path the kernel matches against.
- **FR-104**: WHILE `gui_mode` is disabled THE SYSTEM SHALL grant no access to `~/Library`
  beyond what `shared_paths` names.

### Non-functional

- **NFR-101**: The rules satisfying FR-102 SHALL be the last rules in the generated profile.
  SBPL resolves a path against the last matching rule, so a denial placed before any grant
  is not a denial.

## Interface contract

**Configuration**

| Key | Env var | Type | Default | Description |
| --- | --- | --- | --- | --- |
| `gui_mode` | `SANDME_GUI_MODE` | boolean (env: `1` or `true`) | `false` | Also grants read+write to `~/Library`, `/private/tmp` and `/private/var/folders`. |

No new key, flag or exit code. `gui_mode` already exists in `Config` and in the README; this
spec states what it does and widens it to cover `~/Library`.

## Constraints and dependencies

- macOS Seatbelt (`sandbox-exec -p`), per SPEC-0001 NFR-001. Last-match-wins rule
  resolution is a property of SBPL, not a choice available here.
- FR-004 (SPEC-0001) — `shared_paths` remains the way a user names what they want shared.
  FR-102 is a ceiling on it, not a replacement.

## Success criteria *(mandatory)*

- **SC-101**: With `shared_paths = ["~/"]` and `gui_mode = 1`, a sandboxed process cannot
  create a file under `~/Library/LaunchAgents`, verified by running the command and checking
  both the exit status and the absence of the file.
- **SC-102**: With `gui_mode = 1`, an editor's writes under `~/Library/Application Support`
  succeed.
- **SC-103**: With `gui_mode` off and a `shared_paths` that does not name it, `~/Library` is
  unreachable for both reading and writing.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-101 | `gui_mode_allows_temp_writes`, `grants_the_home_library_only_in_gui_mode` (`src/sandbox.rs`) |
| FR-102 | `denies_writing_a_launch_agent_even_when_the_whole_home_is_shared`, `denies_reading_the_keychains_even_when_the_whole_home_is_shared` (`tests/cli.rs`) |
| FR-103 | `denies_writing_a_launch_agent_even_when_the_whole_home_is_shared`, `denies_reading_the_keychains_even_when_the_whole_home_is_shared` (`tests/cli.rs`) — their scratch home is reached through `/var`, so the denial only bites if it names the resolved path |
| FR-104 | `denies_the_home_library_when_gui_mode_is_off` (`tests/cli.rs`), `grants_the_home_library_only_in_gui_mode` (`src/sandbox.rs`) |
| NFR-101 | `denies_the_launchd_and_keychain_directories_after_every_grant` (`src/sandbox.rs`) asserts the ordering; `denies_writing_a_launch_agent_even_when_the_whole_home_is_shared` proves it against the kernel |

## Assumptions

- A sandboxed editor, IDE or coding agent never needs to write a launchd job or read the
  keychain files directly. Nothing in #12's probes or in the README's recipes contradicts
  this; `security list-keychains` still works under the denial, because `securityd` performs
  the read.
- Users who run a GUI application already set `gui_mode`, so moving the `~/Library` grant
  behind it costs them nothing. The README's Neovim, Zed and coding-agent recipes all set it
  already.

## Open questions

None.

## Implementation tasks

- [x] **T-101** — Move the `~/Library` grant inside the `gui_mode` branch (covers FR-101, FR-104)
- [x] **T-102** — Emit the launchd and keychain denials as the profile's final rules, built
      from the resolved `$HOME` (covers FR-102, FR-103, NFR-101)
- [x] **T-103** — Tests: unit tests for the grant and the rule ordering, integration tests
      driving the binary for each denial (covers FR-101 through NFR-101)
- [ ] **T-104** — Not this spec: the four items under Non-goals remain open in #12 and #10

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-06 | Initial draft, covering issue #12 part 1 and the previously unspecified `gui_mode`. |
