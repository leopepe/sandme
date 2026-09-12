# SPEC-0010: Warn when a security-widening setting comes from the environment

## Metadata

- **Status**: Draft
- **Created**: 2026-09-12
- **Updated**: 2026-09-12
- **Related ADRs**: none yet
- **Related specs**: SPEC-0001 (extends; adds a warning to `config` loading), SPEC-0002
  (`gui_mode`), SPEC-0003 (`allow_private_egress`), SPEC-0007 (the `shared_paths` default)

## Summary

A sandboxed command cannot widen the run it is in, but it can set the terms of the *next* one.
Writing `~/.sandme/config.toml` is already denied (commit 91da40a), yet every setting also has a
`SANDME_*` environment variable, and under the default home share a sandboxed command can write
`~/.zshrc`, `~/.bashrc` or `~/.config/fish/config.fish` to `export SANDME_ALLOW_PRIVATE_EGRESS=1`
or `SANDME_SHARED_PATHS=/`. The user's next `sandme` run then starts pre-widened with nothing
warning them ([#30](https://github.com/leopepe/sandme/issues/30), audit-rated HIGH). This spec
makes sandme emit a loud, `sandme:`-prefixed warning to stderr whenever a security-widening
setting's effective value was sourced from the environment rather than from the config file, so a
pre-widened run cannot pass silently. It is a non-breaking mitigation: the setting still takes
effect, and nothing about the existing precedence changes.

## Problem

Reproduced against the built binary on macOS (Darwin 25.6.0):

- A sandboxed command with the default home share (`shared_paths = ["~/"]`, or any share that
  includes the user's shell rc) appends `export SANDME_ALLOW_PRIVATE_EGRESS=1` to `~/.zshrc`.
- The current run is unaffected — its profile is already loaded. But the user's next `sandme` run
  inherits the variable from their shell, and starts with the proxy relaying to loopback, RFC1918
  and link-local destinations (the pivot SPEC-0003 closed), with no indication that a setting was
  widened for them.
- The same holds for `SANDME_SHARED_PATHS=/` (share the whole filesystem) and `SANDME_GUI_MODE=1`
  (widen the filesystem grants).

Config-file denial does not generalise to the environment: sandme cannot deny writes to every
shell rc without denying most of `$HOME`, which is shared on purpose. The three directions the
issue lists are (1) refuse to honour an env override the profile would let the child write —
heuristic and error-prone; (2) move security-relevant settings to CLI flags env/config cannot set
— the only option that fully closes it, at a usability and compatibility cost; (3) warn loudly.
This spec implements direction (3) now, and records direction (2) under Open questions for the
maintainer, because it is a breaking product decision.

## Goals

- Warn, loudly and unmissably, when a security-widening setting's effective value came from a
  `SANDME_*` environment variable rather than the config file or the default.
- Name, in the warning, the setting and the environment variable that carried it, so the user can
  see exactly what was widened and where to look.
- Keep the mitigation non-breaking: the setting still takes effect, exit status is unchanged, and
  the existing env-over-file-over-default precedence (SPEC-0001 FR-009) is untouched.
- Stay silent when the same value is set in the user's own config file or left at its default,
  so the warning means something and does not train the user to ignore it.

## Non-goals

- Refusing or overriding an env-sourced widening (direction 1). This spec informs; it does not
  block. A warning that also changed behaviour would be a breaking change dressed as a warning.
- Removing or deprecating any `SANDME_*` variable, or moving settings to CLI-only flags
  (direction 2). That closes the hole but is a breaking product decision — see Open questions.
- Warning about non-widening settings. `proxy_port` from the environment is not a security
  concern and MUST NOT warn.
- Detecting *that* a shell rc was written, or which command wrote it. The warning is about the
  value's provenance at load time, not about attributing the write.
- Any change to what the profile or the proxy permits. This spec touches `config` and `main`
  only; it does not touch `src/profile.rs`, so it needs no `/security-audit`.

## User scenarios *(mandatory)*

### Story 1 — A pre-widened run is called out (P1)

As someone who runs `sandme <tool>` after a sandboxed command may have touched my shell rc, I want
to be told when a widening setting is being applied from my environment, so that a silently
pre-widened run cannot pass unnoticed.

**Acceptance scenarios**

1. **Given** `SANDME_ALLOW_PRIVATE_EGRESS=1` set in the environment, **When** sandme runs any
   command, **Then** a `sandme:`-prefixed warning on stderr names `allow_private_egress` and
   `SANDME_ALLOW_PRIVATE_EGRESS`, and the command still runs.
2. **Given** the same value instead written to `~/.sandme/config.toml` with no environment
   override, **When** sandme runs any command, **Then** no widening warning is emitted.

### Story 2 — A broadened share is called out (P2)

As the same user, I want to be warned specifically when `SANDME_SHARED_PATHS` grants access beyond
what my config file or the default already covers, so that a share widened for me stands out while
a narrower or equal env value does not cry wolf.

**Acceptance scenarios**

1. **Given** a config file (or default) confined to a project and `SANDME_SHARED_PATHS=/` in the
   environment, **When** sandme runs, **Then** a warning names `shared_paths` as broadened by the
   environment.
2. **Given** `SANDME_SHARED_PATHS` set only to a subdirectory of the baseline, **When** sandme
   runs, **Then** no widening warning is emitted.

### Edge cases

- **The env variable is present but falsey** (`SANDME_ALLOW_PRIVATE_EGRESS=0`): the value is not a
  widening, so no warning — covered by FR-1002.
- **The same value is in both the file and the environment**: the environment is the source under
  the precedence rule (FR-009), so it warns — covered by FR-1001.
- **A path spelled through a symlink or with an unexpanded `~`** (`/var` vs `/private/var`): the
  comparison is textual (see Assumptions) and errs toward warning, never toward silence.

## Requirements *(mandatory)*

### Functional

- **FR-1001**: WHEN a security-widening setting's effective value is sourced from a `SANDME_*`
  environment variable THE SYSTEM SHALL emit a `sandme:`-prefixed warning to stderr naming the
  setting and the environment variable that carried it. The security-widening settings are
  `allow_private_egress` (via `SANDME_ALLOW_PRIVATE_EGRESS`), `gui_mode` (via `SANDME_GUI_MODE`),
  and `shared_paths` (via `SANDME_SHARED_PATHS`).
- **FR-1002**: IF a security-widening setting's effective value is sourced from the config file or
  left at its built-in default THEN THE SYSTEM SHALL NOT emit that warning. For the boolean
  opt-ins (`gui_mode`, `allow_private_egress`) an environment value that is not the widening value
  (e.g. `0` or `false`) SHALL likewise not warn.
- **FR-1003**: WHEN `SANDME_SHARED_PATHS` grants a path that lies outside every path of the
  file-or-default baseline THE SYSTEM SHALL treat `shared_paths` as environment-sourced widening
  and warn; WHEN the environment value stays within the baseline (equal, or a subdirectory) THE
  SYSTEM SHALL NOT warn.
- **FR-1004**: THE SYSTEM SHALL determine, per security-widening setting, whether its effective
  value came from the config file, the environment, or the built-in default, and base the warning
  on that provenance.
- **FR-1005**: THE SYSTEM SHALL write the warning to stderr only, SHALL NOT write it to stdout,
  and SHALL NOT change the exit status or prevent the command from running.

### Non-functional

- **NFR-1001**: THE SYSTEM SHALL preserve the existing configuration precedence — environment over
  config file over default (SPEC-0001 FR-009). Provenance is observed for the warning only; it
  MUST NOT alter which value takes effect. Verified by the unchanged precedence tests.
- **NFR-1002**: Provenance detection SHALL touch neither the filesystem (beyond the config-file
  read `load` already performs) nor the network, and SHALL add no new runtime dependency, so the
  mitigation adds no measurable startup cost beyond the existing config load.

## Interface contract

**Configuration** *(unchanged — no new key, flag, variable or exit code)*

| Key | Env var | Widening? | Warned when env-sourced |
| --- | --- | --- | --- |
| `shared_paths` | `SANDME_SHARED_PATHS` | when broadened beyond the baseline | yes (FR-1003) |
| `gui_mode` | `SANDME_GUI_MODE` | when `true` | yes (FR-1001) |
| `allow_private_egress` | `SANDME_ALLOW_PRIVATE_EGRESS` | when `true` | yes (FR-1001) |
| `proxy_port` | `SANDME_PROXY_PORT` | no | no |

**Streams**

| Stream | Carries |
| --- | --- |
| stderr | The `sandme: warning: …` line(s), one per env-sourced widening setting |
| stdout | The child's output only — never a warning (posix.md §3, FR-1005) |

**Exit codes / errors**

| Code | Condition | Message to user |
| --- | --- | --- |
| unchanged | a widening setting is env-sourced | `sandme: warning: <setting> came from the environment (<VAR>) …` on stderr; the run proceeds |

## Key entities *(optional)*

- **Provenance**: for a setting, whether its effective value came from the built-in default, the
  config file, or the environment. Recorded for the security-widening settings and consulted to
  decide whether to warn.

## Constraints and dependencies

- Configuration is read in `config.rs` and passed in (docs/guidelines/code/consistency.md §4);
  `config.rs` does not validate paths against the filesystem (docs/guidelines/code/simplicity.md
  §3), so the `shared_paths` broadening check is textual.
- Diagnostics go to stderr with the `sandme:` prefix (docs/guidelines/architecture/posix.md §3);
  the warning is emitted in `main`, keeping `config::load` free of I/O and unit-testable.
- The precedence in SPEC-0001 FR-009 is fixed and must not change.

## Success criteria *(mandatory)*

- **SC-1001**: With `SANDME_ALLOW_PRIVATE_EGRESS=1` in the environment, a `sandme:` warning naming
  the setting and the variable appears on stderr, and the command still runs — verified by running
  the binary.
- **SC-1002**: With the same value in `~/.sandme/config.toml` and no environment override, no
  widening warning appears — verified by running the binary.
- **SC-1003**: The existing precedence and behaviour tests still pass, confirming the mitigation
  changed no value that takes effect.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-1001 | `warns_when_the_environment_enables_private_egress`, `warns_when_the_environment_broadens_shared_paths` (`src/config.rs`); `warns_when_a_widening_setting_comes_from_the_environment` (`tests/cli.rs`) |
| FR-1002 | `stays_silent_when_the_config_file_enables_private_egress`, `stays_silent_when_the_environment_disables_private_egress`, `load_stays_silent_when_the_config_file_enables_private_egress` (`src/config.rs`); `stays_silent_when_the_same_setting_comes_from_the_config_file` (`tests/cli.rs`) |
| FR-1003 | `warns_when_the_environment_broadens_shared_paths`, `stays_silent_when_env_shared_paths_stay_within_the_baseline` (`src/config.rs`) |
| FR-1004 | `provenance_attributes_the_environment_over_the_file`, `provenance_attributes_the_config_file_when_the_environment_is_absent`, `provenance_attributes_the_default_when_neither_sets_it` (`src/config.rs`) |
| FR-1005 | `warns_when_a_widening_setting_comes_from_the_environment` asserts the warning on stderr, the child's line on stdout, and a successful run (`tests/cli.rs`) |
| NFR-1001 | `environment_overrides_previous_values`, `opens_private_egress_only_when_the_environment_asks_for_it` (`src/config.rs`) — precedence unchanged |
| NFR-1002 | Review: provenance detection reads the already-parsed table and the returned env flags; no filesystem, network or dependency is added (`Cargo.toml` unchanged) |

## Assumptions

- A widening value sourced from the environment is worth warning about even when the user set it
  deliberately in a shell profile: the whole point of the mitigation is that the environment is a
  channel a sandboxed command can write, so a false positive (a warning the user does not need) is
  the safe direction, and a false negative (a silent widening) is the dangerous one.
- The `shared_paths` broadening check compares paths as written, without expanding `~` or
  resolving symlinks, because `config.rs` must not touch the filesystem. A path spelled through a
  symlink (`/var` vs `/private/var`) or with an unexpanded `~` that in fact equals the baseline may
  over-warn; it never under-warns. Real users rarely set `SANDME_SHARED_PATHS` to their exact
  current directory (the default already covers it), so the practical false-positive surface is
  small.
- `gui_mode` is treated as a security-widening setting alongside `allow_private_egress` and
  `shared_paths`: it grants read+write to `~/Library` and `$TMPDIR` (SPEC-0007 FR-702), a genuine
  broadening a sandboxed command could plant via `SANDME_GUI_MODE=1`. Including it costs nothing
  beyond the same one-line check and closes the same threat model. `proxy_port` is not widening and
  is excluded.

## Open questions

None blocking this spec's implemented behaviour. Recorded for the maintainer to decide, because it
is a breaking product change and therefore out of scope here:

- **Direction 2 — move security-relevant settings to CLI-only flags.** The warning in this spec
  informs but does not close the hole: an env-sourced widening still takes effect. The only option
  that fully closes it is to make the security-relevant settings (`allow_private_egress`,
  `gui_mode`, and the widening of `shared_paths`) settable *only* by a CLI flag or the config file
  — never by an environment variable the sandboxed child can plant. Concretely: drop
  `SANDME_ALLOW_PRIVATE_EGRESS` and `SANDME_GUI_MODE` from `apply_env`, add `--allow-private-egress`
  and `--gui-mode` flags (posix.md §5: every short option a long form; list-valued
  `--shared-paths a,b`), and keep `SANDME_SHARED_PATHS` only if a warned widening is deemed
  acceptable. This is a breaking change to a documented interface (README recipes and the SPEC-0003
  test `routes_http_egress_through_the_proxy` set `SANDME_ALLOW_PRIVATE_EGRESS`), so it needs the
  maintainer's decision and its own spec with `Spec deltas` against SPEC-0002/0003/0007. Until then
  the warning is the mitigation.

## Implementation tasks

- [x] **T-1001** — Track per-setting provenance (config-file / environment / default) in
      `config.rs`: parse the config file into a table to detect which keys it set, and have
      `apply_env` report which variables it applied (covers FR-1004, NFR-1001, NFR-1002)
- [x] **T-1002** — Compute widening warnings from provenance, including the `shared_paths`
      broadening check against the pre-environment baseline; return them from `load` as data
      (covers FR-1001, FR-1002, FR-1003)
- [x] **T-1003** — Emit the warnings on stderr in `main`, prefixed `sandme:`, without changing the
      exit status or the run (covers FR-1005)
- [x] **T-1004** — Unit tests for provenance and warning selection, and integration tests driving
      the binary for the env-sourced and config-file cases (covers FR-1001–FR-1005)

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-12 | Initial draft: warn loudly on env-sourced widening (issue #30, direction 3). Direction 2 (CLI-only flags) recorded under Open questions for the maintainer. |
