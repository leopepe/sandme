---
name: spec-generation
description: Use when converting a user story or feature request into a formal spec following the project template. Produces a Draft spec in docs/specs/ ready for user review.
---

# Spec generation

Convert a user story into a formal spec (`docs/specs/NNNN-*.md`) following the project template
and the spec-driven development workflow. Report; do not implement.

This skill owns the procedure for writing a spec from a user story. It is invoked by agents
that need to create specs (e.g. `spec-generator`, handover workflows) but does not own the
decision of *when* to write a spec — that belongs to the calling agent or the user.

## Not this skill's job

| Question | Skill that owns it |
| --- | --- |
| Is this change too trivial to need a spec? | `spec-driven-development.md` §9 (fast path) — the calling agent decides |
| Does a spec already exist for this capability? | `spec-check` — it indexes existing specs before generation |
| Is the spec complete and ready to implement? | `spec-review` — it audits specs against code |

## 1. Parse the user story

Extract three elements from the user's request:

- **What:** the capability or feature being requested (one coherent capability).
- **Why:** the user's motivation or the problem being solved.
- **Acceptance criteria:** observable conditions that prove the feature works.

A valid user story has all three. If any is missing, ask the user for clarification before
proceeding. Do not guess.

## 2. Check for existing specs

Before writing, search `docs/specs/` for specs that already cover this capability:

```
ls docs/specs/*.md
```

Read every spec except `0000-spec-template.md`. If an existing spec covers the same capability:

- If it is at `Status: Implemented` or `Accepted`, the capability already exists. Report this to
  the user and stop — do not create a duplicate.
- If it is at `Status: Draft` or `Review`, the capability is already being specified. Report the
  existing spec and ask the user whether to extend it or write a new one.

Only proceed if no existing spec covers this capability, or the user explicitly asks for a new
spec.

## 3. Pick the next NNNN

Find the next free `NNNN` in `docs/specs/`:

```
ls docs/specs/*.md | grep -oE '[0-9]{4}' | sort -n | tail -1
```

Increment by one. Zero-pad to four digits. Numbers are never reused, even if a spec is
withdrawn.

## 4. Write the spec

Copy `docs/specs/0000-spec-template.md` to `docs/specs/NNNN-short-name.md`. Delete the
instruction comment block. Fill every mandatory section:

- **Metadata**: status = Draft, date = today, related ADRs = none (yet), related specs = none.
- **Summary**: two or three sentences describing the feature and its user.
- **Problem**: the user-visible problem motivating the work. No solution language.
- **Goals**: bulleted, verifiable outcomes.
- **Non-goals**: explicit scope boundaries to prevent gold-plating. Be generous.
- **User scenarios**: stories ordered by priority (P1 highest). Each story has Given/When/Then
  acceptance scenarios. Each story must be independently implementable.
- **Requirements**: EARS format (WHEN/IF/THE SYSTEM SHALL), one behaviour per requirement.
  Functional requirements are `FR-NNN`; non-functional are `NFR-NNN`. NFRs MUST give numbers,
  not adjectives.
- **Interface contract**: CLI flags, config keys, exit codes — exact names, defaults, precedence.
- **Success criteria**: measurable outcomes distinct from requirements.
- **Verification**: how each requirement is proven (test name or procedure). Every FR/NFR must
  appear here.
- **Assumptions**: reasonable defaults where the story was silent.
- **Open questions**: every ambiguity as `[NEEDS CLARIFICATION: question]`.
- **Implementation tasks**: ordered, traceable to requirements. Tests come before or with the
  code.

Delete optional sections that do not apply. Never leave a section empty.

## 5. Validate the spec

Before presenting to the user, check:

- Every mandatory section is filled.
- Every requirement is in EARS format and testable.
- Every requirement appears in the Verification table.
- Every `[NEEDS CLARIFICATION]` marker is mirrored under Open questions.
- Non-goals are explicit and generous.
- Implementation tasks cite the requirements they cover.
- The spec is reviewable in one sitting. If it grows past that, split it.

## 6. Present to the user

The spec is `Status: Draft`. Do not implement. Stop and ask for review.

If the user approves the spec, the calling agent sets `Status: Accepted`, resolves open
questions, and begins implementation per `spec-driven-development.md` §3.3. That is not this
skill's job.

## Red flags

| If you catch yourself... | Do this instead |
| --- | --- |
| Guessing an unstated decision | Record it as `[NEEDS CLARIFICATION: question]` and mirror under Open questions |
| Adding features outside the story's scope | Put them in Non-goals — that is the boundary |
| Writing a requirement without a verification row | Add the row, or the requirement does not belong |
| Leaving a section empty | Delete it if optional, or fill it if mandatory |
| Reusing a requirement ID | NNNN is sequential and never reused |
| Implementing code | This skill writes specs, not code. Stop after presenting the spec |
| Writing "fast" or "user-friendly" in an NFR | Give a number: "adds no more than 150 ms to startup" |
| Paraphrasing the user's story | Quote it in the Summary and Problem sections |
