# Agent: spec-generator

## Scope

`project` — converts user stories into specs (`docs/specs/NNNN-*.md`).

## When to Use

- The user provides a user story in the format: what, why, and acceptance criteria.
- The user asks to convert a feature request into a formal spec.
- A requirement needs to be captured as a spec before implementation begins.
- The user runs a command like `/spec <story>` or asks "write a spec for X".

## Procedure

This agent composes one source of truth:

- `spec-generation` skill for the full spec-writing procedure.

### Composition contract

```
sandme --agent spec-generator
  └── delegates spec writing → spec-generation skill (§1–§6)
  └── authority: docs/specs/0000-spec-template.md (project standard)
  └── produces the spec file defined by spec-generation skill
```

Steps the agent performs:

1. **Receive the user story.** The user provides a feature request in the format: what, why,
   and acceptance criteria.
2. **Delegate to `spec-generation` skill.** The skill handles:
   - Parsing and validating the user story (§1).
   - Checking for existing specs that cover the same capability (§2).
   - Picking the next free NNNN (§3).
   - Writing the spec following the template (§4).
   - Validating the spec (§5).
   - Presenting the spec to the user (§6).
3. **Stop.** The spec is `Status: Draft`. Do not implement. The user reviews and approves
   before implementation begins.

## Pitfalls

- Do not implement code — this agent only writes the spec.
- Do not guess unstated decisions — the skill records them as open questions.
- Do not add features outside the story's scope — the skill writes non-goals as the boundary.
- Do not reuse requirement IDs — the skill picks the next free NNNN, sequential and never reused.
- Do not skip the existing-spec check — the skill checks before writing.

## Verification

- The agent produces a complete `docs/specs/NNNN-*.md` file following the template.
- Every mandatory section is filled; no empty sections remain.
- Every requirement is in EARS format and testable.
- Open questions are explicit and collected in one section.
- The spec is presented for review — no code is written.
- The skill's red flags did not fire (no guessing, no scope creep, no missing verification).
