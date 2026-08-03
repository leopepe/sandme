# Quality gates

The commands that MUST pass before code is called done, and before any review is triggered.

Audience: humans and code agents. Rules use MUST / MUST NOT / SHOULD.

## 1. Principle

A reviewer reads code; a compiler reads code faster. Formatting noise, a clippy warning, a build
error or a red test spend a reviewer's attention on what a command answers in seconds.

Run the gate first. A review is for what tooling cannot check.

Gates are run, not assumed. "Should pass" is not a result.

## 2. The gate

| Order | Command | Passes when |
| --- | --- | --- |
| 1 | `cargo fmt` | `cargo fmt --check` then exits `0` — no diff |
| 2 | `cargo clippy --all-targets` | No warnings. Levels are set in `[lints.clippy]`, thresholds in `clippy.toml` |
| 3 | `cargo build` | Compiles with no warnings |
| 4 | `cargo test` | Every test passes; no test ignored without a cited reason |

Run them in that order. Formatting moves the lines clippy reports against, clippy surfaces most
build failures before `cargo build` does, and a test run against code that does not compile
tells you nothing.

`cargo doc --no-deps` MUST also be warning-free — see `docs/guidelines/code/rust.md` §4.

## 3. When the gate runs

- **After writing or changing any code**, before the work is described as done. MUST NOT report a
  task complete without having run all four in that session.
- **Before triggering any review** — `/review-standards`, architecture review, code review, or
  opening a PR. A review requested on unformatted, unlinted, non-compiling or red code MUST be
  refused: the gate is fixed first, then the review is requested again.
- **Before every commit.** A commit that fails the gate is a broken bisect point.
- **Before ticking an implementation task** in a spec, and before `Status: Implemented` — see
  `docs/guidelines/sdd/spec-driven-development.md` §3.3 and §3.4.

A gate failure outranks everything else in progress. Fix it before continuing to the next task.

## 4. Reporting the result

- Report what the command printed. MUST NOT claim a gate passed on the strength of expectation,
  a previous run, or an unrelated change.
- A failure is reported as a failure, with the message, in the same breath as the work it
  belongs to. MUST NOT bury it or describe partial work as complete.
- MUST NOT make a gate pass by silencing it: no `#[ignore]` to turn a red test green, no
  `#[allow]` without a `reason` citing a requirement ID or ADR (see
  `docs/guidelines/code/simplicity.md` §2), no `--no-verify`, no deleting a failing assertion.
- A gate that cannot pass for a reason outside the change — broken toolchain, yanked dependency —
  is stated plainly, with what was tried. That is a blocked task, not a passing one.

## 5. Review checklist

- [ ] `cargo fmt --check` exits `0`.
- [ ] `cargo clippy --all-targets` produces no warnings.
- [ ] `cargo build` compiles with no warnings.
- [ ] `cargo test` passes; no test ignored without a cited reason.
- [ ] `cargo doc --no-deps` produces no warnings.
- [ ] All of the above were run after the last edit, and their output was read.
- [ ] Every `#[allow]` added cites a requirement ID or ADR in a `reason`.
- [ ] Nothing was ignored, silenced or deleted to make the gate pass.

## 6. Related

- `AGENTS.md` — the command list and the finishing conditions for any task.
- `docs/guidelines/tests/testing.md` — what `cargo test` is expected to contain.
- `docs/guidelines/code/simplicity.md` — clippy thresholds and the `#[allow]` escape hatch.
- `docs/guidelines/sdd/spec-driven-development.md` — the stage exit gates this one feeds.
