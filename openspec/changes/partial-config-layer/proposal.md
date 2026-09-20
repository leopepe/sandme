# Partial config layer (R3)

## Why

`src/config.rs` answers one question — *who set this key?* — with four separate mechanisms
(issue [#62](https://github.com/leopepe/sandme/issues/62), finding R3 of the refactoring review
of 2026-09-16, commit `903602a`):

- `FileKeys` (`src/config.rs:277`) and `EnvKeys` (`:305`) are structurally identical: the same
  four `bool` fields, the same `#[allow(clippy::struct_excessive_bools)]` with the same reason.
- The config file is parsed twice — once into a `toml::Table` purely for `contains_key`, then
  again through `try_into` (`:187–192`).
- `Provenance` (`:321`) carries three variants that are only ever compared against
  `Provenance::Environment`, in four places (`:361`, `:370`, `:374`, `:380`).
- Every field states its default twice: once in `impl Default`, once in
  `#[serde(default = "…")]`. The doc comment at `:87` records the bug that came out of exactly
  that duplication — `shared_paths` had a different default depending on whether a config file
  existed.

The module body is 446 lines today (tests start at `:447`), so it is over the 400-line limit in
`docs/guidelines/code/simplicity.md` §2 before a single test, while the `//!` escape-hatch note
claims the overage is tests. Both the duplication and the inaccurate note go away with one
change of shape.

This change serves no new requirement. It preserves, unchanged, SPEC-0001 FR-009 (precedence),
SPEC-0010 FR-1001–FR-1005 and NFR-1001/NFR-1002 (the widening warnings and their provenance
input), and SPEC-0017 FR-1705 (`read_only_paths` as a widening setting). It is structural under
`docs/guidelines/sdd/spec-driven-development.md` §9 — no observable behaviour, interface or
stated requirement changes — with one documentation consequence: SPEC-0010's Verification row
for FR-1004 names three unit tests (`provenance_attributes_the_environment_over_the_file`,
`provenance_attributes_the_config_file_when_the_environment_is_absent`,
`provenance_attributes_the_default_when_neither_sets_it`) whose subject, the `provenance()`
function, is being deleted. That row gets replacement tests that verify the same requirement
against the new shape, so FR-1004 stays verified rather than losing its proof.

## What Changes

- Add one partial `Layer` struct to `src/config.rs`: every field an `Option`, absent meaning
  "this layer said nothing". An `Option` that is `Some` *is* the provenance bit.
- Produce the layer twice — once by deserialising the config file, once by reading the
  `SANDME_*` environment — and merge both onto `Config::default()`. Precedence stays
  environment over file over default (FR-009).
- Remove `FileKeys`, `EnvKeys`, `Provenance`, `provenance()` and `apply_env`.
- Parse the config file once. The `toml::Table` pass that existed only for `contains_key` goes.
- Remove `Deserialize` and every `#[serde(default = "…")]` from `Config`: it is no longer
  deserialised directly, so each default has exactly one definition, in `impl Default`.
- Rewrite `widening_warnings` to read the two layers. `proxy` and `proxy_port` keep no warning:
  that stays an explicit omission in `widening_warnings`, not something the type enforces
  (see design.md).
- Rewrite the `//!` module note to state what is actually true about the file's size once the
  body is back under 400 lines, or to name what is genuinely over the limit if it is not.
- Update the FR-1004 row of SPEC-0010's Verification table to name the tests that replace the
  three `provenance_attributes_*` ones, and add a Changelog line recording the restructure.

Not breaking: no configuration key, environment variable, CLI flag, exit code, stream or
warning text changes.

## Capabilities

### New Capabilities

None. Requirements for this project live in `docs/specs/`, and this change introduces no
requirement.

### Modified Capabilities

None. `skip_specs: true` is set in this change's `.openspec.yaml`: no spec-level behaviour
changes, and inventing a requirement under `openspec/specs/` would contradict the rule that
`docs/specs/` is the single home for requirements.

## Platforms affected

**Both macOS and Linux, identically, and neither backend is touched.** `src/config.rs` sits
above the platform split: it produces the `Config` that `sandbox/seatbelt/` and
`sandbox/landlock/` each consume. The change alters how that `Config` is assembled, not its
fields or their values, so both backends see the same struct they see today and neither is left
inconsistent with the other. No file in the security-critical set
(`src/sandbox/seatbelt/profile.rs`, `src/sandbox/landlock/`, `src/proxy.rs`, `src/egress.rs`,
`src/sandbox/mod.rs`) is edited, so the AGENTS.md review table does not trigger
`/security-audit` for this change.

## Non-goals

- **Changing any effective value, or the precedence that picks it.** FR-009 and NFR-1001 are
  fixed. If a value that takes effect changes, the refactor is wrong.
- **Changing the warning text, the set of warned settings, or which stream carries them.**
  FR-1001–FR-1005 and FR-1705 describe today's behaviour and keep describing it afterwards.
- **Giving `proxy` a provenance warning.** `proxy = false` narrows rather than widens, so it is
  deliberately unwarned today (`src/config.rs:36–44`) and stays unwarned. In the layer shape
  every field gains an `Option`, so the omission becomes a decision `widening_warnings` states
  rather than one the types make for it.
- **Direction 2 of SPEC-0010** — moving security-relevant settings to CLI-only flags
  (issue [#34](https://github.com/leopepe/sandme/issues/34)). Still a breaking product decision
  for the maintainer; untouched here.
- **Splitting `src/config.rs` into a module directory.** The target is one cohesive file back
  under the line budget, not a new module boundary.
- **Injecting the environment for testability.** `load_from` reads `SANDME_*` from the process
  environment today and still will; the existing `serial_test` discipline in the unit tests
  stays as it is.
- **Touching either sandbox backend, the proxy or egress.**

## Impact

| Area | Effect |
| --- | --- |
| `src/config.rs` | The whole change: module body and its unit tests. Roughly 90 lines removed. |
| `docs/specs/0010-warn-env-sourced-widening.md` | FR-1004 Verification row renamed to the replacement tests; Changelog line added. No requirement text changes. |
| `tests/cli.rs` | **Unchanged.** `warns_when_a_widening_setting_comes_from_the_environment` and `stays_silent_when_the_same_setting_comes_from_the_config_file` are the behavioural contract and must pass without edits. |
| Other modules | None. `Config`'s public fields are unchanged, so `main`, `proxy`, `sandbox/seatbelt/profile.rs` and `sandbox/landlock/plan.rs` compile untouched. `Config` loses its `Deserialize` derive; a repo-wide `grep -rn "Deserialize" src/` returns hits only in `src/config.rs:13,20`, so nothing else deserialises it. |
| Dependencies | None added or removed. `toml` and `serde` are both still used — `toml::from_str` once, `serde::Deserialize` on `Layer`. |

### Blast radius searched

- `grep -rn -E "FileKeys|EnvKeys|Provenance|provenance\(" .` (excluding `.git`, `target`,
  `graft`) → 46 hits in `src/config.rs`, 3 in `docs/specs/0010-warn-env-sourced-widening.md`.
  The three spec hits are prose about the *concept* of provenance (lines 135, 137, 167), which
  the change keeps — only the Rust type goes — so they are a recorded non-target, not a task.
  The FR-1004 Verification row (line 197) names the three deleted tests and **is** a task.
- `grep -rn -E "SANDME_SHARED_PATHS|SANDME_READ_ONLY_PATHS|SANDME_GUI_MODE|SANDME_ALLOW_PRIVATE_EGRESS|SANDME_PROXY|SANDME_PROXY_PORT" .`
  → hits in `README.md`, `examples/`, `docs/reference/`, `docs/specs/`, `tests/cli.rs`,
  `src/egress.rs`, `.agents/reports/`. All are a recorded non-target: this change adds, renames
  and removes no configuration key and no environment variable, so the documented surface is
  identical afterwards.
- `grep -rn "config::tests\|config\.rs" docs/specs/*.md` → Verification rows in SPEC-0001
  (FR-008), SPEC-0002 (FR-105), SPEC-0003 (FR-205), SPEC-0007 (FR-701), SPEC-0010 and SPEC-0017
  (FR-1701, FR-1702, FR-1705) name unit tests in `src/config.rs`. Every one of those names is
  kept by this change; only the three `provenance_attributes_*` names, cited by SPEC-0010
  FR-1004 alone, are replaced. Verified by re-running this grep in the verification task.
