# SPEC-0009: Pseudo-terminal allocation

## Metadata

- **Status**: Draft <!-- Draft | Review | Accepted | Implemented | Superseded -->
- **Created**: 2026-09-10
- **Updated**: 2026-09-10
- **Related ADRs**: none yet
- **Related specs**: SPEC-0001 (extends; corrects its device rules), SPEC-0002 (extends; both add profile grants beyond `shared_paths`)

## Summary

A sandboxed command cannot allocate a pseudo-terminal: `openpty(3)` fails with
`out of pty devices` / `openpty: Operation not permitted`. This spec adds the profile grant
that lets it succeed. It is what makes a terminal-driven IDE — Zed's integrated terminal and
its login-shell environment loading — usable under sandme (issue
[#29](https://github.com/leopepe/sandme/issues/29)).

## Spec deltas

### ADDED

- **FR-901**: WHEN a sandboxed command allocates a pseudo-terminal THE SYSTEM SHALL permit the
  allocation to succeed.

### MODIFIED

- **SPEC-0001/FR-002** (the Seatbelt profile realising the sandbox) — was: the device rules
  granted `(allow file-read* file-write* (literal "/dev/ptmx"))` and
  `(allow file-read* file-write* (subpath "/dev/pts"))`. Now: `/dev/ptmx` additionally carries
  `(allow file-ioctl (literal "/dev/ptmx"))`, the slave devices are granted as
  `(allow file-read* file-write* (regex #"^/dev/ttys[0-9]+$"))`, and the `/dev/pts` rule is
  removed. Reason: on macOS the slave PTY is `/dev/ttysNNN`, not the Linux `/dev/pts/N`, and
  unlocking a slave issues `TIOCPTYGRANT`/`TIOCPTYUNLK` on the multiplexer, which needs
  `file-ioctl`. The three rules were bisected in #29 — read-write on `/dev/ptmx` alone, the
  ioctl alone, and the ttys rule alone each still fail; all three together succeed.

### REMOVED

- **`/dev/pts` device rule** (withdrawn) — `(allow ... (subpath "/dev/pts"))` grants nothing on
  macOS: the path does not exist. sandme is macOS-only (SPEC-0001/NFR-001), so a rule kept only
  for a hypothetical Linux port is speculative (`docs/guidelines/code/simplicity.md` §4) and is
  removed rather than carried.

## Problem

Zed launches under sandme and is usable, but its integrated terminal is dead and its
environment is not loaded — Zed reads shell environment by running a login shell *on a
pseudo-terminal*. Both symptoms have one cause: the sandbox denies PTY allocation. On macOS 26
(Darwin 25.6.0, Apple silicon), with the real generated profile (`shared_paths = ["~/",
"/opt/homebrew"]`, `gui_mode = true`):

```
$ sandme 'python3 -c "import pty; pty.openpty()"'
OSError: out of pty devices          # outside the sandbox: PTY OK
```

The shipped profile grants `/dev/ptmx` read-write and `(subpath "/dev/pts")`. `/dev/pts` is the
Linux convention; it does not exist on macOS, where the slave is `/dev/ttysNNN`. Allocation also
issues `TIOCPTYGRANT`/`TIOCPTYUNLK` on `/dev/ptmx`, which the profile never permitted because it
granted no `file-ioctl`. So no PTY can be created, and every terminal-driven feature that depends
on one fails.

## Goals

- A sandboxed command can allocate a pseudo-terminal, so a terminal-driven IDE is usable.
- The profile names the slave devices at their real macOS paths, and states the PTY grant
  honestly as the capability it is.

## Non-goals

These surfaced alongside the PTY problem in #29 and are deliberately left out so this spec stays
reviewable in one sitting:

- **`git` failing under `/var/folders` with `Operation not permitted`.** macOS `/usr/bin/git` is
  an `xcrun` shim that writes a cache into `$TMPDIR` (under `/private/var/folders`). This is a
  configuration interaction, not a code defect: with `gui_mode = true` the temp grant already
  covers it and `git` works; with `gui_mode` off it fails, and the message names `xcrun_db`
  rather than sandme, which makes the diagnosis hard. It overlaps the `gui_mode` temp-grant
  narrowing tracked in #12 §3 — narrowing that grant wrongly reintroduces exactly this failure.
  This spec does not touch the `gui_mode` temp grant. The workaround is to run with `gui_mode`
  on; see the Open questions.
- **Reaching a real git remote.** SSH remotes (port 22) are denied by design — the profile
  permits egress only through the proxy (SPEC-0003). HTTPS remotes fall through to a username
  prompt (the credential helper produces nothing, then prompts with no PTY). Whether SSH egress
  is in scope is a design decision worth its own issue; this spec neither implements nor decides
  it.
- **Gating PTY allocation on `gui_mode`.** A command's need for a terminal is unrelated to
  whether it is a GUI application, so the grant is unconditional, like the other device rules.

## User scenarios *(mandatory)*

### Story 1 — A terminal works inside the sandbox (P1)

As someone running a terminal-driven editor or shell under sandme, I want the sandboxed command
to open a pseudo-terminal, so that its integrated terminal runs and its login-shell environment
loads.

**Acceptance scenarios**

1. **Given** a sandboxed command that calls `openpty(3)` (directly, or via `/usr/bin/script`, or
   `python3 -c "import pty; pty.openpty()"`), **When** it runs under sandme, **Then** the
   allocation succeeds and the command runs on the pseudo-terminal instead of failing with `out
   of pty devices`.
2. **Given** a sandboxed command that runs a login shell on a pseudo-terminal
   (`pty.spawn(["/bin/zsh","-l","-c","echo ENV-VIA-PTY"])`), **When** it runs under sandme,
   **Then** the shell's output reaches the caller.

### Edge cases

- **`gui_mode` off.** PTY allocation does not depend on the GUI grants, so Story 1 holds with
  `gui_mode` on or off.

## Requirements *(mandatory)*

### Functional

- **FR-901**: WHEN a sandboxed command allocates a pseudo-terminal THE SYSTEM SHALL permit the
  allocation to succeed, whether or not `gui_mode` is set.

### Non-functional

- **NFR-901**: THE PTY grant SHALL extend to no path beyond the pseudo-terminal multiplexer
  (`/dev/ptmx`) and the slave devices matching `^/dev/ttys[0-9]+$`, and SHALL add no network
  reach.
- **NFR-902**: THE `file-ioctl` grant SHALL cover `/dev/ptmx` only, never a slave device. This is
  the load-bearing restriction: `TIOCSTI` (line-injection) and `TIOCSCTTY` (controlling-terminal
  seizure) on a foreign terminal are what a slave-device `file-ioctl` grant would enable, and both
  MUST stay denied. SBPL cannot restrict the slave `file-read*`/`file-write*` grant to
  self-allocated nodes, so a residual remains (see the Known limitation); the withheld slave
  `file-ioctl` is what keeps it below an escape.

## Interface contract

No CLI, configuration, or exit-code surface changes. The change is entirely within the generated
Seatbelt profile; a user observes it only as PTY-using commands no longer failing.

## Constraints and dependencies

- macOS Seatbelt / SBPL (SPEC-0001/NFR-001). The slave-device grant is a `regex` rule; its SBPL
  string `#"^/dev/ttys[0-9]+$"` must survive Rust string escaping intact.
- The three rules are interdependent (see the MODIFIED delta): the implementation must add all
  three, and a regression test must fail if any one is dropped.

## Success criteria *(mandatory)*

- **SC-901**: On macOS 26, `sandme '/usr/bin/script -q /dev/null /bin/echo pty-works'` prints
  `pty-works` and exits `0`; the same invocation without the fix prints `openpty: Operation not
  permitted`.
- **SC-902**: The profile contains no `/dev/pts` rule.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-901 | `allocates_a_pseudo_terminal` (`tests/cli.rs`) — runs `/usr/bin/script`, which calls `openpty(3)`, under the built binary and asserts it succeeds; `grants_pseudo_terminal_allocation` (`src/profile.rs`) asserts both the `file-ioctl` and the `/dev/ttysNNN` regex rules are present. Manually confirmed with the issue's own probes (`pty.openpty()` and `pty.spawn` of a login shell) under `SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew"`. |
| NFR-901 | `grants_pseudo_terminal_allocation` pins the grant to `/dev/ptmx` and `^/dev/ttys[0-9]+$`; the existing egress tests (`tests/cli.rs`) show no network rule is added. |
| NFR-902 | `grants_pseudo_terminal_allocation` asserts `file-ioctl` names `/dev/ptmx` only and no slave device; `/security-audit` (2026-09-11) PoC-confirmed `TIOCSTI` on a foreign slave is denied. |

## Assumptions

- The slave devices this macOS version hands out match `^/dev/ttys[0-9]+$`. This is the BSD
  naming (`/dev/ttys000`…); verified on Darwin 25.6.0.

## Known limitation

The `^/dev/ttys[0-9]+$` grant matches *every* slave on the host, not only the ones this sandbox
allocated — SBPL has no predicate for "a node this process created". A sandboxed process can
therefore open a `/dev/ttysNNN` held by another same-uid process outside the sandbox and read from
or write to that terminal. Confirmed by `/security-audit` (2026-09-11), graded **Medium**: it is a
bounded read/write on same-uid terminals, not code execution — `TIOCSTI` line-injection and
`TIOCSCTTY` are denied because `file-ioctl` is granted on `/dev/ptmx` only (NFR-902). Closing the
residual would need a mechanism SBPL does not offer; it is recorded here rather than left for the
next reader to rediscover.

## Open questions

<!-- Must be empty before Status leaves Draft. -->

- [ ] Should the `gui_mode`-off `git`/`xcrun` failure under `/private/var/folders` be addressed
  here or folded into the #12 §3 temp-grant work? — does not block FR-901.
- [ ] Is SSH egress (port 22) in scope for sandme at all? — does not block FR-901; belongs in its
  own issue.

## Implementation tasks

- [x] **T-901** — Add the failing tests: `allocates_a_pseudo_terminal` (`tests/cli.rs`) and
  `grants_pseudo_terminal_allocation` (`src/profile.rs`) (covers FR-901, NFR-901).
- [x] **T-902** — Add `(allow file-ioctl (literal "/dev/ptmx"))` and
  `(allow file-read* file-write* (regex #"^/dev/ttys[0-9]+$"))` to `generate_profile`, remove the
  `/dev/pts` rule, and document why in the function's rustdoc (covers FR-901, NFR-901, SC-902).

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-10 | Initial draft. |
