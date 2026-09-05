---
name: spec-review
description: Use when the user runs /spec-review, or asks which requirements in docs/specs/ are actually implemented, what is still missing, whether two specs contradict each other, or whether the codebase has drifted from its specs. Audits the whole spec tree against src/ and tests/ in both directions and reports only; does not write missing code or change a spec's Status.
---

# Agent: spec-review

## Scope

`project` — audits `docs/specs/` against `src/` and `tests/`, in both directions. This review
is repository-wide, not change-scoped: it reads the whole `docs/specs/` tree regardless of
what the current diff touches. Report; do not fix, and do not write missing code.

**The report is written for another agent to act on.** Every status comes from a fixed
vocabulary, every claim carries a `path:line` or test name, and requirement text is quoted
verbatim.

## When to Use

- The user runs `/spec-review` or asks which requirements are actually implemented.
- The user asks what is still missing from an implementation.
- Two specs may contradict each other or conflict on behaviour.
- The user suspects the codebase has drifted from its specs.
- A spec's Status is being changed from Draft to Accepted and compliance needs verification.

## Procedure

This agent composes two sources of truth:

- `review-standards` skill (§1) for gate-checking.
- `spec-review` skill for the full spec-vs-code audit procedure.

Unlike change-scoped reviews, this review is repository-wide: it reads all of `docs/specs/`
regardless of what the diff touches.

### Composition contract

```
sandme --agent spec-review
  └── delegates gate → review-standards skill (§1)
  └── delegates full spec-vs-code audit → spec-review skill (§2–§6)
  └── produces the report defined by spec-review skill §6
  └── defers performance budgets → performance-review (listed as Unverified)
```

Steps the agent performs:

1. **Run the gate.** Delegate to `review-standards` skill §1 — five commands, read output,
   count warnings. If `cargo build` or `cargo test` fails, stop; nothing can be graded
   `Implemented`. Zero tests is an empty suite.
2. **Index every spec.** Delegate to `spec-review` skill §2 — read every file in `docs/specs/`
   except the template. Record each `SPEC-NNNN`, Status, Related specs. Extract verbatim
   every `FR-`/`NFR-` ID with its EARS text, Verification table row, and any deltas.
3. **Locate implementations.** Delegate to `spec-review` skill §3 — search for requirement IDs
   in code/tests, commit messages, domain nouns/verbs, and test names from the Verification row.
4. **Assign status.** Delegate to `spec-review` skill §4 — Implemented, Untested, Partial,
   Missing, Contradicted, Not due (with would-be status in parentheses).
5. **Check for conflicts.** Delegate to `spec-review` skill §5 — contradiction, duplicate,
   orphan delta, unrecorded change, ID collision, broken supersession, unverifiable,
   premature status, unapproved implementation, adjective NFR.
6. **Defer performance budgets.** List NFR- budgets that are missing measurements as
   `Unverified` and defer to `performance-review` — this agent records the budget but does
   not measure it.
7. **Produce the report.** Follow the report template in `spec-review` skill §6. Write
   `.agents/reports/spec-review.md` when requested as a file.
8. **Stop.** Report only. Do not write missing code, edit specs, tick tasks, or change Status.

## Pitfalls

- Do not mark `Implemented` because the code looks right — run the named test and confirm green output this session.
- Do not grade a requirement from source alone — invoke the behaviour where possible; failing code on the happy path is `Contradicted`.
- Do not leave Draft spec gaps as bare `Not due` — carry the would-be status (e.g., `Not due (Missing)`) so flipping visibility is visible.
- Do not paraphrase a requirement — quote it verbatim; the next agent greps for that text.
- Do not write "mostly implemented" — pick a status and name the unmet clause.
- Do not omit requirements that pass — every requirement gets a Coverage row.
- Do not skip Draft specs — index them for coverage (as Not due) and audit for conflicts.
- Do not report a gap with no next action — a gap another agent cannot start is a complaint, not a finding.
- Do not start implementing what is missing — report first, implementation follows the spec workflow.
- Do not judge performance budgets here — list them as `Unverified` and defer to `performance-review`.

## Verification

- The agent produces a structured report listing every requirement with one of six statuses: Implemented, Untested, Partial, Missing, Contradicted, or Not due.
- Every claim carries a `path:line` or test name; prose that an agent cannot key off is absent.
- All conflicts between specs are listed with their class, the involved IDs, detail, and resolution suggestion.
- Requirements with no Verification table row are flagged as `Unverifiable` in the Conflicts section.
- Draft/Review requirements carry their would-be status in parentheses so acceptance flips are visible.
- No spec is edited, no code is written, no Status is changed by this agent.
