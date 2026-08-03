---
name: spec-review
description: Use when the user runs /spec-review, or asks which requirements in docs/specs/ are actually implemented, what is still missing, whether two specs contradict each other, or whether the codebase has drifted from its specs.
---

# Spec review

Audit `docs/specs/` against `src/` and `tests/`, in both directions:

1. **Coverage** — for every `FR-`/`NFR-` in every spec, does code implement it and does a test
   prove it?
2. **Conflicts** — do two specs bind the same behaviour to different outcomes, or declare deltas
   against requirements that do not exist?

Repository-wide, not change-scoped: this review reads the whole `docs/specs/` tree regardless of
what the current diff touches. Report; do not fix, and do not write the missing code.

**The report is written for another agent to act on.** Every status comes from a fixed vocabulary,
every claim carries a `path:line` or a test name, and requirement text is quoted verbatim. Prose
that an agent cannot key off is noise — cut it.

## Not this skill's job

| Question | Skill that owns it |
| --- | --- |
| Does the current change follow `docs/guidelines/`? | `review-standards` |
| Is the structure sound, does it contradict an ADR? | `architecture-review` |
| Does it meet an `NFR-` **performance** budget? | `performance-review` — this skill records the budget as `Unverified` and defers the measurement |
| Does the code simply not work — wrong output, wrong format, a crash? | This skill owns it when it breaks a requirement: that is `Contradicted`, and it is found by **running** the behaviour, not reading it |

## 1. Run the gate first

`Implemented` is a claim about tests that pass. Run the gate — the five commands and their pass
criteria are in `.agents/skills/review-standards/SKILL.md` §1.

If `cargo build` or `cargo test` fails, **stop**. Report the failure. Nothing can be graded
`Implemented` against a tree that does not compile or is red. A `fmt`, `clippy` or `doc` failure
alone is recorded in the report header and the audit continues.

`cargo test` reporting `0 passed; 0 failed` is not a green suite — it is an **empty** one. Say so
in the header, and remember that no requirement can then reach `Implemented`.

## 2. Index the specs

```
ls docs/specs/*.md
```

Read every one except `0000-spec-template.md`. For each, record: `SPEC-NNNN`, `Status`, and the
`Related specs` line. Then extract, verbatim:

- every `FR-`/`NFR-` ID and its EARS text;
- its row in the **Verification** table (or the absence of one);
- every entry under **Spec deltas** (ADDED / MODIFIED / REMOVED), and any `(withdrawn)` or
  `(superseded by …)` marker.

`Accepted` and `Implemented` specs are the contract. `Draft` and `Review` specs are candidates —
audit them for conflicts and hygiene, but a `Draft` requirement that is unimplemented is
`Not due`, not a gap.

## 3. Locate the implementation

For each requirement, in this order — stop at the first that answers:

1. `rg -n 'FR-007|NFR-002' src/ tests/ docs/` — IDs cited in code, tests, or task lists.
2. `git log --oneline --grep='FR-007'` — the commit that claims it.
3. The domain nouns and verbs in the requirement text: `rg -n 'proxy_port' src/`.
4. The test named in the spec's Verification row: `rg -n '<test name>' tests/ src/`.

Then confirm the test actually passes, by name, and read the output:

```
cargo test <test name> -- --exact
```

A Verification row naming a test that does not exist makes the requirement `Untested`, whatever
the code does — record the named test and its absence in the Evidence column. A test
that exists but was not run this session cannot be reported as passing —
`docs/guidelines/code/quality-gates.md` §4.

## 4. Status vocabulary

One value per requirement. Do not invent a sixth.

| Status | Means | Requires |
| --- | --- | --- |
| `Implemented` | Code satisfies it and a named test proves it | `path:line` + a test that ran green this session |
| `Untested` | Code satisfies it; no passing test names it | `path:line`, and what the Verification row says |
| `Partial` | Some of the stated behaviour exists | `path:line` + the clause of the requirement that is not met |
| `Missing` | Nothing in `src/` implements it | The searches from §3 that came back empty |
| `Contradicted` | Code does something the requirement forbids or differs from | `path:line` + the requirement clause it breaks |
| `Not due` | The spec is `Draft`/`Review`, so nothing is owed yet | The spec's Status, **and the status it would take on an `Accepted` spec** |

`Not due` is the one status that hides work. A `Draft` spec whose proxy does not exist reads as
clean, and flipping the status turns it red without a line of code changing. So every `Not due`
row carries its would-be status — `Not due (Missing)` — and the Result line counts them
separately. A reader deciding whether to accept the spec needs the second number.

Status is decided by **running** the behaviour, not by reading the code. Code that looks correct
and fails on the happy path is `Contradicted`, not `Implemented` — and that difference is the
single most valuable thing this review produces. Where a requirement describes something you can
invoke, invoke it and paste what it printed.

## 5. Conflict classes

Check every pair of specs that touch the same capability, plus every spec against itself.

| Class | Test |
| --- | --- |
| `Contradiction` | Two requirements bind the same trigger to different behaviour (`WHEN X … SHALL A` vs `WHEN X … SHALL B`) with neither marked withdrawn or superseded |
| `Duplicate` | Two IDs state the same behaviour — one of them should be a delta, not a second requirement |
| `Orphan delta` | A **Spec deltas** entry cites an ID that does not exist, or one already withdrawn |
| `Unrecorded change` | A spec changes behaviour an `Implemented` spec defines, with no **Spec deltas** section — `spec-driven-development.md` §4 |
| `ID collision` | The same `FR-`/`NFR-` ID appears in two specs, or a number was reused after withdrawal — §4, IDs are never reused |
| `Broken supersession` | A spec claims `supersedes`, but the superseded spec's requirements are unmarked and its Status is unchanged |
| `Unverifiable` | A requirement has no row in its own Verification table — §5 |
| `Premature status` | `Status` has left `Draft` with **Open questions** non-empty — §3.3 |
| `Unapproved implementation` | Code in `src/` implements a spec still at `Draft`/`Review` — §8, "MUST NOT write implementation code for a change that has no spec at `Accepted`". Name the requirement IDs already built and the open questions that were silently answered |
| `Adjective NFR` | An `NFR-` states a quality with no number — §5 |

## 6. Report

Emit exactly these sections, in this order. When the user asks for the report as a file, or it is
being handed to another agent, write it to `.agents/reports/spec-review.md` — one file,
overwritten each run. It is a snapshot of a moving target, not an archive.

```markdown
## Spec review

**Gate:** fmt ✓ · clippy ✓ 0 warnings · build ✓ 0 warnings · test ✓ <N> tests ran · doc ✓
**Specs indexed:** 3 (Draft 1 · Accepted 1 · Implemented 1)
**Requirements indexed:** 24 (FR 19 · NFR 5)
**Deferred:** NFR-001, NFR-003 — performance budgets, measured by `performance-review`

### Coverage
| Requirement | Spec | Status | Implementation | Verified by | Evidence |
| --- | --- | --- | --- | --- | --- |
| FR-001 | SPEC-0001 | Implemented | `src/main.rs:31-48` | `tests/cli.rs::runs_command_sandboxed` | passed `cargo test` this session |
| FR-004 | SPEC-0001 | Untested | `src/sandbox.rs:12` | — | Verification names `denies_unshared_path`; no such test exists |
| FR-009 | SPEC-0001 | Missing | — | — | `rg 'FR-009' src/ tests/` and `rg 'proxy_port'` both empty |
| FR-005 | SPEC-0002 | Not due (Missing) | — | — | Spec is `Draft`; `src/proxy.rs` is a comment. Becomes `Missing` on `Accepted` |

Every requirement gets a row. Nothing is omitted for being unremarkable.

### Gaps
#### G1 — FR-009 · `Missing`
**Requirement:** "<verbatim EARS text>"
**Searched:** <the §3 searches run, and their results>
**What exists instead:** <nearest behaviour in src/, or "nothing">
**Next action:** <the concrete task, phrased so another agent can start it — file, function, test>

### Conflicts
| # | Class | Specs / IDs | Detail | Resolution |
| --- | --- | --- | --- | --- |
| C1 | Contradiction | SPEC-0001/FR-006 vs SPEC-0002/FR-101 | Config precedence reversed | Write the delta in SPEC-0002 per §4, or withdraw FR-101 |

### Unspecified behaviour
| Location | Behaviour | Requirement |
| --- | --- | --- |
| `src/proxy.rs:88` | Retries the upstream connection three times | none — unspecified, or fast path per §9 |

### Result
<Implemented N · Untested N · Partial N · Missing N · Contradicted N · Not due N (of which M become Missing on Accepted) · Conflicts N> — <one sentence>
```

## 7. Stop

Report only. Do not write the missing code, do not edit a spec, do not tick an implementation task
or change a `Status`. Those are separate, requested steps that follow
`docs/guidelines/sdd/spec-driven-development.md` §3.3.

## Red flags

| If you catch yourself... | Do this instead |
| --- | --- |
| Marking `Implemented` because the code looks right | Run the named test. Green output, this session, or it is `Untested` |
| Grading a requirement from the source alone | Invoke the behaviour. Convincing code that fails on the happy path is `Contradicted` |
| Leaving a `Draft` spec's gaps as bare `Not due` | Carry the would-be status: `Not due (Missing)`. Otherwise the flip looks free |
| Paraphrasing a requirement | Quote it verbatim — the next agent greps for that text |
| Writing "mostly implemented" | Pick a status from §4 and name the clause that is unmet |
| Omitting requirements that pass | Every requirement gets a Coverage row |
| Skipping a spec because it is `Draft` | Index it: `Not due` for coverage, still fully audited for conflicts |
| Reporting a gap with no next action | A gap another agent cannot start on is not a finding, it is a complaint |
| Starting to implement what is missing | Report first. Implementation follows the spec workflow, not this review |
| Judging performance from the code | `NFR-` budgets are deferred to `performance-review`, listed as `Unverified` |
