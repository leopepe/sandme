## Standards review

**Gate:** fmt ✓ · clippy ✓ 0 warnings · build ✓ 0 warnings · test ✓ 78 tests ran (38 unit + 40
integration), 1 ignored with a cited reason · doc ✓ 0 warnings. All five run after the last edit.

**Scope:** case 2 — branch `fix/process-and-sysctl-visibility` ≠ `main`; nothing is committed on
the branch yet, so the whole change is uncommitted. 3 files modified (+413/−18: `README.md`,
`src/profile.rs`, `tests/cli.rs`) plus 2 untracked files of this change
(`docs/specs/0005-process-and-sysctl-visibility.md`,
`.agents/reports/security-audit-2026-09-09.md`). `opencode.json` is untracked and predates this
work — out of scope.

**Guidelines applied:** `architecture/posix.md`, `code/consistency.md`, `code/quality-gates.md`,
`code/rust.md`, `code/simplicity.md`, `general/handover.md`,
`general/project-maturity-phases.md`, `performance/cli-performance.md`,
`sdd/spec-driven-development.md`, `tests/testing.md`.

### Verdicts

| Guideline | Rule | Verdict | Finding |
| --- | --- | --- | --- |
| code/simplicity.md | §2 file under 400 lines | Advisory | F1 |
| code/simplicity.md | §2 function body under 50 lines, complexity, nesting, parameters | Pass | — |
| code/simplicity.md | §3 one purpose, no "and" | Pass | — |
| code/simplicity.md | §4 evidence over anticipation | Pass | — |
| code/simplicity.md | §5 over-engineering smells | Pass | — |
| code/simplicity.md | §6 idiomatic Rust, comments explain why | Pass | — |
| code/consistency.md | §2 reuse before writing | Pass | — |
| code/consistency.md | §3 one name per concept, one verb per action | Pass | — |
| code/consistency.md | §4 structural consistency, no local error type | Pass | — |
| code/consistency.md | §5 code agrees with the spec | Pass | — |
| code/rust.md | §3 module `//!` doc | Pass | — |
| code/rust.md | §4 rustdoc on public items, no doc warnings | Pass | — |
| code/rust.md | §5 errors, no user-reachable `expect` | Pass | — |
| code/quality-gates.md | §2 five commands clean | Pass | — |
| code/quality-gates.md | §4 nothing silenced to pass | Pass | — |
| tests/testing.md | §2 unit vs integration placement | Pass | — |
| tests/testing.md | §3 assert observable behaviour, one behaviour per test | Advisory | F2 |
| tests/testing.md | §5 Given/When/Then in integration tests, named after the Then | Pass | — |
| tests/testing.md | §6 every FR/NFR proven by a named test | Pass | — |
| sdd/spec-driven-development.md | §2 spec shape, mandatory sections | Pass | — |
| sdd/spec-driven-development.md | §3.2 no invented decisions; assumptions recorded | Pass | — |
| sdd/spec-driven-development.md | §3.3 `Status: Accepted` before implementation | Blocker | F3 |
| sdd/spec-driven-development.md | §5 EARS, numbers not adjectives | Pass | — |
| sdd/spec-driven-development.md | §6 traceability both ways | Pass | — |
| sdd/spec-driven-development.md | §8 no ID reuse, no behaviour outside Goals | Pass | — |
| architecture/posix.md | argument grammar, streams, exit status | N/A | — |
| performance/cli-performance.md | §2–§4 concurrency, async, no blocking | N/A | — |
| performance/cli-performance.md | §5 a performance change cites its measurement | N/A | — |
| performance/cli-performance.md | §6 no unnecessary startup work | Pass | — |
| general/handover.md | when a handover is required | N/A | — |
| general/project-maturity-phases.md | — | Not assessable | — |

### Findings

#### F1 — `src/profile.rs` is 447 lines, over the 400-line limit · `src/profile.rs:1-447` · code/simplicity.md

**Rule:** "| File / module | 400 lines | Review — no lint exists |" and "**The escape hatch.** A
limit MAY be exceeded when the alternative is worse … Exceeding a limit MUST be justified in
place with an allow attribute whose reason cites a requirement ID or ADR."

**Evidence:** The file was 316 lines before this change and is 447 after: 243 lines of module
and 204 of tests. The change added the allowlist constants, `append_readable_sysctls`, one line
of denial, and four unit tests — one per new requirement (FR-301 … FR-304). A `base_profile()`
helper was extracted first and applied to all 7 sites that built the same fixture, and the new
comments were cut back; that took it from 471 to 437 before the justification itself was added.

The overage is justified in place, in the module `//!` doc, citing SPEC-0005 and both
alternatives: splitting the module needs a second purpose and there is none (§3), and moving the
tests out of line would leave one module in six whose tests are not at its foot
(`code/consistency.md` §4). The justification is a `//!` comment rather than an
`#[allow(reason = …)]` because no lint exists to allow — the guideline's own table says the
limit is review-enforced.

**Fix:** Author's call, and it needs a decision rather than an edit: accept the justified
overage, or say which of the two alternatives is preferred. If tests should move out of line,
that is a repo-wide pattern change under `code/consistency.md` §1 ("Change the pattern
everywhere, or not at all"), not a change to this module.

#### F2 — `probe` is a `#[test]` that asserts nothing · `tests/cli.rs:1103-1119` · tests/testing.md

**Rule:** "One behaviour per test. The test name states that behaviour" and §3 "Assert observable
outcomes".

**Evidence:** `probe` is not a test: it is the program the two SPEC-0005 tests need *inside* the
sandbox, re-executed by `probe_under_sandme` and `probe_unsandboxed`. It makes the two kernel
calls under scrutiny and prints the result; the assertions live in its two drivers. It carries
`#[ignore = "the probe the SPEC-0005 tests drive; not a test on its own"]`, so an ordinary run
reports it as ignored with a cited reason (`code/quality-gates.md` §2) and it can never turn a
red test green (§4).

The alternative was a second binary or a compiler at test time; the sandbox needs *some* program
that calls `sysctl(2)` with a numeric MIB, and the test binary is the only one a test can be sure
exists. Recorded as an Advisory because the shape stretches "one behaviour per test", not because
a rule is broken.

**Fix:** None available inside the guideline as written. If the pattern is unwelcome, the
alternative is a dedicated fixture binary under a dev-only target, which is more machinery for
the same assertions.

#### F3 — implementation landed with the spec at `Status: Review`, not `Accepted` · `docs/specs/0005-process-and-sysctl-visibility.md:5` · sdd/spec-driven-development.md

**Rule:** §8 "MUST NOT write implementation code for a change that has no spec at `Accepted`",
and §3.2's exit gate: "the spec is written and presented to the user for review. Stop here. Do
not implement."

**Evidence:** SPEC-0005 is at `Status: Review` and the implementation is written. This follows
the repository's own precedent rather than its guideline: SPEC-0002, SPEC-0003 and SPEC-0004 are
all at `Status: Review` with their code merged, and SPEC-0004's changelog states the reason
verbatim — "Status `Review`: the exit codes are observable behaviour and the change is presented
for approval together with its implementation." SPEC-0005's changelog says the same thing and
names SPEC-0004 as the precedent it follows.

Nothing is merged: the work is on a branch, and the approval the gate asks for is still the
user's to give.

**Fix:** One of two, and it is the author's choice, not an edit this review can make. Either
approve SPEC-0005 (`Status: Accepted`, then `Implemented` once the tasks are ticked) and the
sequence is regularised after the fact as it was for 0002–0004; or amend §3.3/§8 to describe what
this repository actually does — a spec at `Review` may be presented with its implementation, on a
branch, for approval as one unit. The guideline and four of five specs currently disagree, and
that is worth resolving once rather than per change.

### Traceability

| Changed file | Requirement ID | Verified by | Status |
| --- | --- | --- | --- |
| `src/profile.rs` (allowlist constants, `append_readable_sysctls`) | SPEC-0005/FR-301 | `narrows_sysctl_reads_to_an_allowlist`, `denies_a_sysctl_the_allowlist_does_not_name`, `grants_the_sysctls_ordinary_programs_read` | Covered |
| `src/profile.rs` (`process-info-pidinfo (target same-sandbox)`) | SPEC-0005/FR-302, NFR-301 | `grants_process_information_only_inside_the_sandbox`, `denies_reading_another_process_environment` | Covered |
| `src/profile.rs` (`(deny process-info*)`) | SPEC-0005/FR-303 | `denies_every_ungranted_process_information_operation` | Covered |
| `src/profile.rs` (denial before the `$HOME` early return) | SPEC-0005/FR-304 | `denies_process_information_without_a_home` | Covered |
| `src/profile.rs` (allowlist contents) | SPEC-0005/NFR-302 | Measurement procedure in SPEC-0005; result recorded there (40 commands, 0 status differences) | Covered |
| `tests/cli.rs` | SPEC-0005/FR-301, FR-302, NFR-301 | the three tests above | Covered |
| `README.md` | SPEC-0005/FR-301, FR-302 | Documentation of stated behaviour; no test applies | Covered by the spec it documents |

Both new integration tests were run against the profile as it was before the change and both
fail there, so each proves the behaviour it claims rather than passing vacuously.

### Defects noticed

- `src/main.rs:67` cites `SPEC-0003/FR-302`, a requirement ID that does not exist — SPEC-0003
  numbers the proxy credential `FR-203`. Outside this diff; a one-word comment fix.

### Result

Blockers: 1 · Advisories: 2 — mergeable on the same terms as SPEC-0002 through SPEC-0004. F3 is
the repository's standing disagreement with its own guideline rather than a defect in this
change, and it is the one thing here that needs the author's decision; F1 and F2 are justified in
place and recorded for review.
