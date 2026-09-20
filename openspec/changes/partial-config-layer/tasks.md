# Tasks

Numbering is flat so `depends-on` can reference a task by a single number. Every task carries
two annotation lines: `depends-on:` (task numbers that must finish first, or `none`) and
`touches:` (the files it creates or modifies; files it only reads are not listed). A task that
needs a file outside its declared `touches:` is a signal to stop and report, not to widen the
set silently.

What was task 1 in the previous draft is now three tasks run in order — a characterisation-tests
task, then the two halves of the old task 1 (design.md D9): task 2 ("1a") is plumbing only and
must leave every existing test, including the three `provenance_attributes_*` tests, passing
unedited; task 3 ("1b") is the warning rewrite, and is where the security-relevant behaviour
actually changes shape.

**TMPDIR note for this machine:** on the machine this change was planned on, the shell's
`TMPDIR` is `/private/tmp`, which makes the unit test
`gui_mode_grants_the_per_user_temp_dir_only` (`src/sandbox/seatbelt/profile.rs:442`) fail
spuriously — it is not a defect in this change or in that test. Before running `cargo test` (or
the full quality gate) in any task below, run:

```
export TMPDIR=$(getconf DARWIN_USER_TEMP_DIR)
```

## Tasks

- [ ] 1. Characterisation tests, written and passed against the code as it stands **before**
      task 2 changes anything (resolves F2, F3, F7 in
      `.conductor/partial-config-layer/reviews/phase3-contrarian.md`). These assert what
      `src/config.rs` already does today — they are not new behaviour, and every one of them
      must pass, unmodified, on the pre-refactor code before task 2 begins. They are the net the
      rest of this change rides on:
      - A `load_from`-level baseline test (F2 — nothing today exercises the baseline computed
        inside `load_from` itself; the four existing broadening unit tests take
        `baseline_shared`/`baseline_read_only` in as explicit arguments and never touch the line
        that decides what the baseline *is*). Give a config file `shared_paths =
        ["<dir>/project"]` and set `SANDME_SHARED_PATHS=/` in the environment; assert a warning
        naming `shared_paths`. Add the mirror: the same file setting with the environment either
        unset or set to a value inside that baseline; assert `loaded.warnings.is_empty()`.
        Suggested names: `load_warns_when_the_environment_broadens_shared_paths_beyond_the_file_baseline`,
        `load_stays_silent_when_the_environment_value_stays_within_the_file_baseline`.
      - `warns_when_the_environment_enables_gui_mode` and
        `stays_silent_when_the_environment_disables_gui_mode` (F3 — FR-1001 names `gui_mode`
        explicitly, `grep -n "gui_mode" src/config.rs` inside `mod tests` returns only incidental
        default assertions today, and SPEC-0010's Verification table cites nothing for it, yet
        the branch at `src/config.rs:370` is about to be rewritten blind). The second is
        `SANDME_GUI_MODE=0` over a file-set `gui_mode = true` — FR-1002's falsey-env clause.
      - `stays_silent_when_the_environment_sets_proxy_or_proxy_port` (F7 — the fence for
        design.md D2 that an in-code comment cannot provide on its own): set `SANDME_PROXY=1`
        and `SANDME_PROXY_PORT=9999`; assert `loaded.warnings.is_empty()`.
      Follow the existing `load_warns_when_the_environment_enables_private_egress` /
      `load_stays_silent_when_the_config_file_enables_private_egress` pattern for the
      `load_from`-level tests: `#[serial_test::serial]`, an injected temp `home` directory
      (never the process's real `HOME`), `unsafe { std::env::set_var / remove_var }` for the
      `SANDME_*` variables this test owns, and directory cleanup at the end.
      Verify: all four (six, counting the two mirror cases) tests pass against the working tree
      exactly as it stands now — run `cargo test --lib` and paste the output — before task 2
      changes a single line; then the full quality gate, in order: `cargo fmt`,
      `cargo clippy --all-targets`, `cargo build`, `cargo test` (see the TMPDIR note above;
      paste what each command printed).
      depends-on: none
      touches: src/config.rs

- [ ] 2. Task 1a — plumbing only (resolves F8; design.md D1, D3, D4, D5, D9): add `Layer` with
      all six fields `Option` and `Deserialize` derived; add `Layer::from_environment()` reading
      the six `SANDME_*` variables through the unchanged `env_bool` / `split_list`; add
      `Config::merge(self, &Layer) -> Self`; rebuild `load_from` as `Config::default()`, merge
      the file layer into `file_merged`, take the FR-1003 baselines
      (`baseline_shared`/`baseline_read_only`) off `file_merged` by cloning, then merge the
      environment layer onto `file_merged` for the final `config` (design.md D4 — do **not**
      collapse this into the single chained expression `Config::default().merge(&file).merge(&env)`;
      that discards the intermediate value the baseline is taken from and silently kills the
      `shared_paths`/`read_only_paths` warnings with every test still green); delete the double
      `toml::Table` parse and `apply_env`; drop `Deserialize` and every
      `#[serde(default = "…")]` from `Config` so `impl Default` is the one definition of each
      default. Do **not** touch `FileKeys`, `EnvKeys`, `Provenance`, `provenance()` or
      `widening_warnings` — keep all five byte-for-byte. Derive the two key-sets from the layers
      instead of from the `toml::Table` and `apply_env`'s return value, e.g.
      `FileKeys { shared_paths: file.shared_paths.is_some(), read_only_paths:
      file.read_only_paths.is_some(), gui_mode: file.gui_mode.is_some(), allow_private_egress:
      file.allow_private_egress.is_some() }`, and the equivalent for `EnvKeys` from the env
      layer. Do not edit any test in this task, including the four characterisation tests added
      in task 1 and the three `provenance_attributes_*` tests.
      Verify: `grep -rn -E "apply_env|toml::Table" src/config.rs` returns no hits;
      `grep -rn -E "FileKeys|EnvKeys|Provenance|provenance\(" src/config.rs` still returns hits
      (unchanged in count from before this task); `git diff` of `src/config.rs`'s `mod tests`
      block is empty — every existing test, including the three `provenance_attributes_*` tests
      and task 1's four characterisation tests, passes unedited; and the quality gate is green,
      in order: `cargo fmt`, `cargo clippy --all-targets`, `cargo build`, `cargo test` (see the
      TMPDIR note above; paste what each command printed).
      depends-on: 1
      touches: src/config.rs

- [ ] 3. Task 1b — the warning rewrite (resolves F8; design.md D2, D5, D6): delete `FileKeys`,
      `EnvKeys`, `Provenance` and `provenance()`; rewrite `widening_warnings` to read the two
      layers directly, with an in-place comment naming `proxy` and `proxy_port` as deliberately
      unwarned and citing FR-1001 plus FR-1705 as the closed list of warned settings (design.md
      D2); keep the `config.gui_mode &&` conjunct in the `gui_mode` check, but write its comment
      to the corrected rationale in design.md D5 — the falsey case is blocked by
      `Some(false) != Some(true)`, not by this conjunct, which is redundant given that the
      environment merges last; the conjunct stays only for readability, and the comment must not
      claim it is what blocks `SANDME_GUI_MODE=0`; replace the three `provenance_attributes_*`
      tests with the one table-driven test named in design.md D6
      (`attributes_each_widening_warning_to_the_layer_that_set_it`: 4 warned keys × 3 states
      (file-only, env-only, neither) in one loop, 12 cases); rewrite the four tests that lose
      `apply_env` as their subject (`environment_overrides_previous_values`,
      `disables_the_proxy_when_the_environment_asks_for_it`,
      `enables_the_proxy_for_a_true_ish_spelling`,
      `opens_private_egress_only_when_the_environment_asks_for_it`) to call
      `Config::default().merge(&Layer::from_environment())`, keeping their names and
      Given/When/Then comments; keep every other test name cited by a spec Verification row
      verbatim; keep the four characterisation tests added in task 1 — they now exercise the
      rewritten code path and must keep passing.
      Verify: `grep -rn -E "FileKeys|EnvKeys|Provenance|provenance\(" src/config.rs` returns no
      hits; the characterisation tests from task 1 still pass; the two contract tests
      `warns_when_a_widening_setting_comes_from_the_environment` and
      `stays_silent_when_the_same_setting_comes_from_the_config_file` in `tests/cli.rs` pass
      without being edited; and the quality gate is green, in order: `cargo fmt`,
      `cargo clippy --all-targets`, `cargo build`, `cargo test` (see the TMPDIR note above; paste
      what each command printed).
      depends-on: 2
      touches: src/config.rs

- [ ] 4. Make the `//!` module note in `src/config.rs` state what is actually true about the
      file's size (design.md D8): measure the module body with
      `grep -n "mod tests" src/config.rs`; if the body is under 400 lines, rewrite the note so
      its claim that the overage is tests is accurate; if it is not, rewrite it to name what is
      genuinely over the limit and why splitting is the worse alternative
      (`docs/guidelines/code/simplicity.md` §2). Verify: the measured line number is quoted in
      the task report, the note matches it, and the quality gate is green, in order:
      `cargo fmt`, `cargo clippy --all-targets`, `cargo build`, `cargo test` (see the TMPDIR note
      above).
      depends-on: 3
      touches: src/config.rs

- [ ] 5. Keep SPEC-0010 traceable: in `docs/specs/0010-warn-env-sourced-widening.md`, replace the
      three `provenance_attributes_*` test names in the FR-1004 Verification row with
      `attributes_each_widening_warning_to_the_layer_that_set_it` (design.md D6), and add a
      Changelog line recording that the provenance mechanism was restructured to a partial
      config layer (issue #62, R3) with no requirement change — noting that `T-1001`'s "parse
      the config file into a table" wording describes the original implementation, not the
      current one. Change no requirement text, no `Status`, and no other spec. Verify: every
      test name in the file's Verification table exists in `src/config.rs` or `tests/cli.rs`
      (check each with `grep -rn "fn <name>" src/ tests/`), and `git diff --stat docs/specs/`
      lists this file alone.
      depends-on: 3
      touches: docs/specs/0010-warn-env-sourced-widening.md

- [ ] 6. Verification sweep across the whole change, no edits of its own: confirm
      `git diff --stat tests/cli.rs` is empty (the behavioural contract was not touched);
      re-run `grep -rn "config::tests\|config\.rs" docs/specs/*.md` and confirm every unit-test
      name cited by SPEC-0001 (FR-008), SPEC-0002 (FR-105), SPEC-0003 (FR-205), SPEC-0007
      (FR-701), SPEC-0010 and SPEC-0017 (FR-1701, FR-1702, FR-1705) resolves to a test that
      exists; re-run the full quality gate from a clean tree, in order — `cargo fmt`,
      `cargo clippy --all-targets`, `cargo build`, `cargo test` (see the TMPDIR note above) —
      and paste the output; then run `/review-standards`. `/security-audit` is not triggered: no
      file in the security-critical set (`src/sandbox/seatbelt/profile.rs`,
      `src/sandbox/landlock/`, `src/proxy.rs`, `src/egress.rs`, `src/sandbox/mod.rs`) is in any
      task's `touches:`. The `spec-review` agent is not triggered either: task 5 changes a
      Verification row and the Changelog, not a requirement, a delta or a `Status`. Report
      anything either check finds; fixing it is a separate step.
      depends-on: 1, 2, 3, 4, 5
      touches: none
