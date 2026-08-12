# Agent: spec-generator

## Scope

`project` — converts user stories into OpenSpec format (`docs/specs/NNNN-*.md`).

## When to Use

- The user provides a user story in the format: what, why, and acceptance criteria.
- The user asks to convert a feature request into a formal spec.
- A requirement needs to be captured as an OpenSpec before implementation begins.
- The user runs a command like `/spec <story>` or asks "write a spec for X".

## Procedure

This agent is standalone — it does not compose skills.

- **Authority:** Spec template `docs/specs/0000-spec-template.md` (project standard)
- **No gate-checking:** This agent writes specs, not code. The gate does not apply.
- **No guideline delegation:** This agent does not delegate to `review-standards` because it
  produces output (specs), not code reviews.

### Composition contract

```
sandme --agent spec-generator
  └── standalone: no skill composition
  └── authority: docs/specs/0000-spec-template.md (project standard)
  └── produces the spec file defined below (no skill template)
```

Steps the agent performs:

1. **Parse the user story.** Extract:
   - **What:** the capability or feature being requested.
   - **Why:** the user's motivation or the problem being solved.
   - **Acceptance criteria:** observable conditions that prove the feature works.
2. **Validate the input.** A valid user story has:
   - A clear "what" (one coherent capability).
   - A clear "why" (user problem or motivation).
   - At least one acceptance criterion (observable, testable).
   - If any of these are missing, ask the user for clarification.
3. **Write the spec.** Create `docs/specs/NNNN-short-name.md` (next free NNNN):
   - Copy the template from `docs/specs/0000-spec-template.md`.
   - Fill **Metadata**: status = Draft, date = today, related ADRs = none (yet).
   - Fill **Summary**: two sentences describing the feature and its user.
   - Fill **Problem**: the user-visible problem motivating the work.
   - Fill **Goals**: bulleted, verifiable outcomes.
   - Fill **Non-goals**: explicit scope boundaries to prevent gold-plating.
   - Fill **User scenarios**: stories ordered by priority (P1 highest), each with Given/When/Then acceptance scenarios.
   - Fill **Requirements**: EARS format (WHEN/IF/THE SYSTEM SHALL), one behaviour per requirement.
   - Fill **Interface contract**: CLI flags, config keys, exit codes — exact names, defaults, precedence.
   - Fill **Success criteria**: measurable outcomes distinct from requirements.
   - Fill **Verification**: how each requirement is proven (test name or procedure).
   - Fill **Assumptions**: reasonable defaults where the story was silent.
   - Fill **Open questions**: every ambiguity as `[NEEDS CLARIFICATION: question]`.
   - Fill **Implementation tasks**: ordered, traceable to requirements.
4. **Present to the user.** The spec is Draft — do not implement. Stop and ask for review.

## Pitfalls

- Do not implement code — this agent only writes the spec.
- Do not guess unstated decisions — record them as assumptions or open questions.
- Do not add features outside the story's scope — non-goals are your boundary.
- Every requirement must be testable — EARS format, one behaviour per requirement.
- Open questions must be empty before the spec can leave Draft status.
- Do not reuse requirement IDs — NNNN is sequential, never reused.
- The spec must be reviewable in one sitting — if it grows past that, split it.

## Verification

- The agent produces a complete `docs/specs/NNNN-*.md` file following the template.
- Every mandatory section is filled; no empty sections remain.
- Every requirement is in EARS format and testable.
- Open questions are explicit and collected in one section.
- The spec is presented for review — no code is written.
