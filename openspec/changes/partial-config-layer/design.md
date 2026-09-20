# Design: one partial config layer

## Context

See proposal.md — Why. The mechanics that matter for the approach, read off
`src/config.rs` at commit `cf9a469`:

- `load_from(home)` (`:174`) builds the result in four steps: `Config::default()`, then a file
  read that parses the content twice (`toml::Table` for `contains_key`, then `try_into` for the
  struct, `:187–192`), then `apply_env(&mut config)` which mutates the struct and returns
  `EnvKeys`, then `widening_warnings(&config, file, env, &baseline_shared, &baseline_read_only)`.
- The baselines (`:197–198`) are clones of `config.shared_paths` / `config.read_only_paths`
  taken *after* the file is applied and *before* the environment is — the file-or-default
  baseline FR-1003 compares against.
- Four widening keys carry provenance bits: `shared_paths`, `read_only_paths`, `gui_mode`,
  `allow_private_egress`. `proxy` and `proxy_port` carry none.
- `Config` is deserialised directly, so each field states its default twice (`impl Default` at
  `:74` and `#[serde(default = "…")]` on the field). The comment at `:87` records the
  `shared_paths` bug that came from those two definitions disagreeing.

Constraints: `config` is the only module that reads files or the environment
(`src/AGENTS.md`); `config.rs` does not touch the filesystem for path validation
(`docs/guidelines/code/simplicity.md` §3); a file is over budget at 400 lines (§2), and the
escape hatch requires the justification in place to be accurate.

## Goals / Non-Goals

**Goals:**

- One shape — `Layer` — produced twice, so "the file said nothing about this key" and "the
  environment said nothing about this key" are the same fact expressed the same way.
- `load_from` reads as the precedence rule it implements: `Config::default()`, merge file, merge
  environment.
- Every default has exactly one definition, and the class of bug the `:87` comment describes is
  no longer expressible because nothing derives a second set.
- The module body under 400 lines without moving code out of the file.

**Non-Goals:** (design-level, beyond proposal.md — Non-goals)

- No trait, generic or macro over the field set. Six fields written out twice (`Config`,
  `Layer`) is the readable form; a derive to generate one from the other is an abstraction with
  one call site, which `docs/guidelines/code/simplicity.md` §5 rejects.
- No change to `load_from`'s signature or to `Loaded`. `main` is untouched.
- No change to how the environment is read (process `env::var`, not injected).

## Decisions

### D1 — `Option` is the provenance bit; `Layer` carries all six fields, not just the four widening ones

```rust
#[derive(Debug, Default, Deserialize)]
struct Layer {
    shared_paths: Option<Vec<String>>,
    read_only_paths: Option<Vec<String>>,
    proxy: Option<bool>,
    proxy_port: Option<u16>,
    gui_mode: Option<bool>,
    allow_private_egress: Option<bool>,
}
```

`Some` means this layer set the key; `None` means it said nothing. That is exactly what
`FileKeys`/`EnvKeys` encoded as a parallel `bool`, except it now travels with the value it
describes and cannot drift out of step with it.

*Alternative rejected:* keep `Layer` to the four widening keys and go on mutating `Config` for
`proxy` and `proxy_port`. That preserves two mechanisms — a merge for some keys, a mutation for
others — which is the finding restated in a smaller font.

**What D1 does not prevent (C5):** the `Option` removes the *drift* between a value and its
provenance bit, not the possibility of a wrong merge. `merge` still has to be written per field,
and a field forgotten in `merge` compiles and silently ignores that layer. Nothing in the type
system catches that; the unit tests in D6 are what catch it — specifically
`keeps_defaults_for_keys_a_config_file_omits` and `environment_overrides_previous_values`, which
assert per-key outcomes rather than one struct-level equality.

### D2 — `proxy` and `proxy_port` stay unwarned, as an explicit statement in `widening_warnings`

This is the subtlety the issue asks to carry over deliberately. Today `proxy` has no provenance
bit *because the struct has no field for it* — the omission is enforced by `FileKeys`/`EnvKeys`
having four fields. In the layer shape every key gets an `Option`, so the bit for `proxy` and
`proxy_port` now exists and is simply not consulted.

That is the intended design, not an oversight, and it must read as one: `proxy = false` narrows
(no proxy is started and the sandbox grants no egress, `src/config.rs:36–44`), and SPEC-0010's
Non-goals exclude `proxy_port` explicitly ("`proxy_port` from the environment is not a security
concern and MUST NOT warn"). So `widening_warnings` gets a comment naming both keys and why
neither is checked, citing FR-1001's enumeration (`shared_paths`, `gui_mode`,
`allow_private_egress`) and FR-1705 (`read_only_paths`) as the closed list of warned settings.

The type is deliberately not made to enforce this. A `WideningLayer`/`NarrowingLayer` split, or
a marker trait over the warned fields, would encode the policy in the type at the cost of a
second shape — the thing this change exists to remove. The decision stays where a reader
looking for "what warns?" goes: the body of `widening_warnings`.

### D3 — `Config` loses `Deserialize`; `Layer` gains it

`Config` is no longer produced by serde, so every `#[serde(default = "…")]` attribute goes and
`impl Default` is the one definition of each default. `default_shared_paths`,
`default_proxy_port`, `default_proxy`, `default_gui_mode` and `default_allow_private_egress`
either stay as functions called from `impl Default` or become literals inline; the
`default_shared_paths` function stays regardless, because it computes `current_dir` and is
asserted directly by `default_share_is_the_current_directory` (SPEC-0007 FR-701).

`Layer` derives `Deserialize` with no field attributes at all: a missing key in a
`Option<T>` field deserialises to `None` without `#[serde(default)]`, which is the whole point.

*Verified as safe:* `grep -rn "Deserialize" src/` returns two hits, both in `src/config.rs`
(the `use` at `:13` and the derive at `:20`), so no other module deserialises `Config`.
`grep -rn "Config {" src/ tests/` shows the other construction sites (`src/proxy.rs:386`,
`src/sandbox/landlock/plan.rs:239`, `src/sandbox/seatbelt/profile.rs:356,421,633`) are struct
literals in tests, unaffected by a derive coming off — a `Deserialize` derive has no bearing on
a struct literal either way.

Note their shape, because it is easy to get wrong: only `src/proxy.rs:388` uses
`..Config::default()`. The other four spell out **every field exhaustively**. That costs nothing
here, since this change alters no field of `Config` — but it is why the proposal's
"`Config`'s public fields are unchanged" is load-bearing rather than incidental: adding or
removing a field would fail to compile at those four sites, and that is a constraint on any
follow-up, not on this change.

### D4 — `merge` consumes the layer field by field and is the only place precedence lives

```rust
impl Config {
    fn merge(mut self, layer: &Layer) -> Self { /* one `if let Some(v) = &layer.f` per field */ }
}

let file_merged = Config::default().merge(&file);
let baseline_shared = file_merged.shared_paths.clone();
let baseline_read_only = file_merged.read_only_paths.clone();
let config = file_merged.merge(&env);
```

**Do not collapse this back into `Config::default().merge(&file).merge(&env)`.** That single
chained expression is one statement shorter, reads the same in a diff, and passes every test in
this change — but it discards `file_merged`, the only place the FR-1003 baseline can be taken
from. Compute the baseline from the final `config` instead (the only value left) and
`is_broadened(&config.shared_paths, &config.shared_paths)` compares a slice to itself: always
`false`. `shared_paths` and `read_only_paths` stop warning, silently, with `cargo test` green and
both `tests/cli.rs` contract tests still passing — nothing in the declared verification catches
it (see D5).

Applied file-then-environment, the environment wins wherever both set a key — FR-009 unchanged,
and now stated in one line instead of spread across `try_into` plus six `if let Ok(...)` blocks
in `apply_env`.

*Alternative rejected:* `Layer::merge(self, other) -> Layer` (merge the layers first, then apply
one layer to the defaults). It needs the same per-field code and then still has to know, at the
end, which layer each `Some` came from for the warnings — the merged layer has lost that. Two
merges onto `Config` keeps `file` and `env` separately available for `widening_warnings`, which
is what FR-1004 needs.

### D5 — the FR-1003 baseline is `Config::default().merge(&file)`

The baseline stays "file-or-default", computed before the environment layer is merged, exactly
as `:197–198` does today. In the new shape it is the intermediate value of the merge chain, so
it is taken by cloning `shared_paths` / `read_only_paths` off the file-merged `Config` before
the second `merge`. No change to `is_broadened`, `is_within` or `normalize`.

`widening_warnings` then takes `(&config, &file, &env, &baseline_shared, &baseline_read_only)` —
five parameters, the `too-many-arguments-threshold` in `clippy.toml`, so it is at the limit and
not over it. If a later field pushes it over, the baselines get grouped into a small struct;
that is not needed now.

Each of the four checks becomes the direct form the issue names, e.g.:

```rust
if config.gui_mode && env.gui_mode == Some(true) { … }
if env.shared_paths.is_some() && is_broadened(&config.shared_paths, baseline_shared) { … }
```

For the booleans, `env.gui_mode == Some(true)` is totally equivalent to today's
`config.gui_mode && provenance(...) == Environment` — the truth table over all five reachable
states (file absent/`false`/`true` × env absent/`false`/`true`) agrees on all of them, not just
the common ones. But the reason first drafted here for keeping the `config.gui_mode &&` guard was
false. The falsey case — `SANDME_GUI_MODE=0` over a file-set `gui_mode = true`, FR-1002's
falsey-env clause — is blocked by `Some(false) != Some(true)` alone, not by the guard. The
environment is merged last, so whenever `env.gui_mode == Some(true)` holds, `config.gui_mode` is
already `true`: the guard can never be the conjunct that decides the outcome. It is redundant
given the merge order, full stop.

The conjunct is kept anyway, but for an honest reason: it keeps the condition legible as "the
effective value is the widening one, and the environment carried it," so a reader is not forced
to re-derive the merge-order argument above just to see why dropping it would still be safe.
Keeping it with the *original* rationale is the hazard, not the conjunct itself: a future editor
who believes the guard does the blocking work may "simplify" `env.gui_mode == Some(true)` to
`env.gui_mode.is_some()`, trusting the guard to still catch the falsey case — and it will not,
because it never did. The comment at this check states the true reason (merge order, not the
guard) so that edit does not look safe to make.

### D6 — the three `provenance_attributes_*` tests are replaced by one table-driven test, not three separate ones

SPEC-0010's Verification table cites them for FR-1004, so deleting them would leave a
requirement unverified. The first draft of this decision replaced them one-for-one with three
new tests. That does not hold up: two of the three duplicate coverage that already exists per
key (e.g. `stays_silent_when_the_config_file_sets_read_only_paths`,
`stays_silent_when_the_config_file_enables_private_egress`), and `provenance()` was *one* rule
shared by all four warned keys. Inlining it into `widening_warnings` turns that one rule into
four separate copies of `env.X.is_some()` / `env.X == Some(true)` — and a one-for-one test
replacement would leave three of those four copies with no test touching them at all.

The replacement is one parameterised test, table-driven over the four warned keys
(`shared_paths`, `read_only_paths`, `gui_mode`, `allow_private_egress`) crossed with the three
states each layer can be in — file-only `Some`, env-only `Some`, neither set — 4 × 3 = 12 cases
run through one loop body against `widening_warnings`:

| State | file layer | env layer | Expected |
| --- | --- | --- | --- |
| file-only | `Some(widened value)` | `None` | silent (FR-1002) |
| env-only | `None` | `Some(widened value)` | warns, naming the key (FR-1004) |
| neither | `None` | `None` | silent (default) |

Proposed test name, to be reused verbatim in the spec's Verification row:
`attributes_each_widening_warning_to_the_layer_that_set_it`. It gives `provenance()`'s single
rule the per-key coverage it never had, rather than four inlined copies with three of them
untested.

This is not a 13th cell short of the old "environment over file" case (both layers `Some`, env
wins): which *value* wins when both layers set a key is FR-009 merge precedence, already
exercised by `environment_overrides_previous_values` (kept verbatim below). What this loop
verifies is attribution — which layer earns credit for a `Some`, and therefore whether a warning
fires at all — and file-only / env-only / neither is the complete state space for that question,
one key at a time.

This tests attribution through `widening_warnings`, the only consumer provenance ever had, so
FR-1004 is verified by its observable effect rather than by a helper that no longer exists.

Every other unit-test name cited by a spec Verification row is kept verbatim, including the four
that lose `apply_env` as their subject (`environment_overrides_previous_values`,
`disables_the_proxy_when_the_environment_asks_for_it`,
`enables_the_proxy_for_a_true_ish_spelling`,
`opens_private_egress_only_when_the_environment_asks_for_it`); their bodies are rewritten to
`Config::default().merge(&Layer::from_environment())` while their names and Given/When/Then
comments stay.

### D7 — behaviour that must survive the rewrite, key by key

These are the edges the current code has and the rewrite has to keep; each is already covered by
a named unit test except where noted.

| Edge | Today | After |
| --- | --- | --- |
| `SANDME_PROXY_PORT` unparseable as `u16` | `if let Ok(port) = port.parse()` — ignored, default stands | `Layer.proxy_port = None`; same outcome. Not covered by a named test today; no new one is added, matching the existing coverage |
| `SANDME_*` boolean spelling | `env_bool`: `1`/`true` case-insensitive, else false | `env_bool` unchanged, wrapped in `Some(...)` |
| Comma list parsing | `split_list`: trim, drop empties | unchanged |
| `shared_paths = []` written in the file | key present → provenance bit set, value empty | `Some(vec![])` → same |
| A type error in a known key (`proxy_port = "x"`) | `try_into` fails → `SandmeError::ConfigParse` | `toml::from_str::<Layer>` fails on `Option<u16>` → same error variant |
| An unknown key in the file | ignored (no `deny_unknown_fields`) | ignored |
| No `HOME` | `config_path(None)` → `./config.toml` | unchanged (SPEC-0014 notes this is undocumented behaviour) |

The type-error row is settled, not an open risk: `reports_a_malformed_config_with_the_reserved_status`
(`tests/cli.rs:897`) asserts only the message prefix, not the error's internal shape, so the
span-carrying `toml::from_str::<Layer>` error still satisfies it — same variant, same exit code
125. Verified by reading that assertion, not assumed by analogy.

### D8 — the `//!` note is rewritten to whatever is then true

The acceptance criterion is "module body back under 400 lines, **or** the `//!` escape-hatch note
rewritten to match what is actually over the limit". The removal is estimated at ~90 lines
against a 446-line body, which would land near 356 — but that is an estimate, not a measurement
(C4: the count has not been taken, because the code does not exist yet). The task therefore
*measures* the body after the rewrite (`grep -n "mod tests" src/config.rs`) and writes the note
that matches the number found: if the body is under 400, the note says the overage is tests and
is then accurate; if it is not, the note names what is actually over and why splitting is the
worse alternative, per `docs/guidelines/code/simplicity.md` §2.

### D9 — task 1 splits into 1a (plumbing) and 1b (the warning rewrite), with a green gate between

The first draft kept this as one task, on the reasoning that both halves live in the same file
and its unit tests sit with it. That reasoning does not hold: 1a and 1b both touch only
`src/config.rs`, so `touches:` is identical for the two halves, and the tasks that follow (the
`//!` note, the spec traceability update, the verification sweep) are already sequential on this
work regardless of whether it lands as one task or two. A `touches:`-based argument for keeping
two tasks merged requires their touched sets to differ; here they do not, so it gives no reason
either way.

The actual reason to split is a gate, not a file boundary:

- **1a — plumbing only.** Add `Layer`, `Layer::from_environment`, `Config::merge`; rebuild
  `load_from` as `Config::default()` → merge file → take the FR-1003 baselines off that
  intermediate `Config` (D4) → merge environment; delete the double `toml::Table` parse and
  `apply_env`; drop `Deserialize` and every `#[serde(default = "…")]` from `Config`. `FileKeys`,
  `EnvKeys`, `Provenance`, `provenance()` and `widening_warnings` stay byte-for-byte, with the
  two key-sets derived from the layers instead of from a `toml::Table` and `apply_env`'s return
  value. Its acceptance condition is the quality gate green with **every existing test
  unedited**, the three `provenance_attributes_*` tests included. That is the whole point: it
  proves the merge and the baseline faithful against the full, unmodified suite the module
  already has, before a single line of the security-relevant warning logic changes.
- **1b — the warning rewrite.** Only once 1a's gate is green: delete
  `FileKeys`/`EnvKeys`/`Provenance`/`provenance()`, inline the four checks against the layers,
  add the D2 comment, and land the D6 table-driven test.

1b is small and is the part worth a reviewer's full attention — it is where the security-relevant
behaviour actually changes shape. 1a is mechanical, and is what makes 1b reviewable on its own
merits rather than bundled with "and also trust that the merge is correct."

## Risks / Trade-offs

- **A field forgotten in `merge` silently ignores that layer** → per-key assertions in the
  existing tests (`keeps_defaults_for_keys_a_config_file_omits` covers file-side omissions,
  `environment_overrides_previous_values` the env side), plus the six-field table in D7 walked
  during review. The compiler does not help here (D1).
- **The warning behaviour is the security-relevant part and it is being rewritten** → the two
  integration tests in `tests/cli.rs` (`warns_when_a_widening_setting_comes_from_the_environment`,
  `stays_silent_when_the_same_setting_comes_from_the_config_file`) are the contract and are
  edited in no task. They exercise the real binary end to end: the warning on stderr, the child's
  output on stdout, a successful exit. What they alone do **not** force — the `shared_paths`
  broadening path computed inside `load_from`, the `gui_mode` falsey-env clause, and silence on
  `proxy`/`proxy_port` — is closed by the characterisation-tests task, which writes and passes
  those cases against the pre-refactor code *before* task 1a touches anything, and by the D6
  table-driven test once 1b lands.
- **`clippy::struct_excessive_bools` on `Layer` — settled, not a risk.** The lint's own
  implementation counts fields whose type `is_bool()`; `Option<bool>` is not `bool`, so it will
  not fire on `Layer`'s three `Option<bool>` fields. The `#[allow(clippy::struct_excessive_bools)]`
  with the FR-1705 `reason` is deleted along with `FileKeys`/`EnvKeys` in 1b, not carried forward
  onto `Layer` "just in case." `cargo clippy --all-targets` still has to stay green, as it does
  for every task; this bullet only says what to expect from it.
- **`clippy::pedantic` on the new `merge`** → `pedantic` is `warn` and a warning is a Blocker
  (`docs/guidelines/code/quality-gates.md`). `merge(mut self, layer: &Layer) -> Self` is the
  shape chosen partly to avoid `needless_pass_by_value` on the layer; if another pedantic lint
  fires, the fix is the code, not an `allow`.
- **The estimated line saving may not reach under 400** → D8 makes the note follow the
  measurement instead of the estimate, so the criterion is met either way.
- **SPEC-0010's implementation-task text (`T-1001`, "parse the config file into a table") becomes
  a historical description of a mechanism that no longer exists** → the spec's Changelog gets a
  line saying so. The task text itself is left as the record of what was done at the time;
  requirements, not tasks, are the contract.

## Migration Plan

None required at runtime: no configuration key, environment variable, file format, flag or exit
code changes, so an existing `~/.sandme/config.toml` and an existing shell environment behave
identically before and after. Rollback is `git revert` of the single commit; nothing persists
state across the change.

## Open Questions

None. The one judgement call — whether `proxy` should gain a warning now that a bit exists for
it — is resolved as "no" in D2, on SPEC-0010's own Non-goals and the narrowing argument in
`src/config.rs:36–44`.
