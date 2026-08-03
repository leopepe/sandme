# SPEC-NNNN: Short feature name

<!--
HOW TO USE THIS TEMPLATE

1. Copy to `docs/specs/NNNN-short-feature-name.md` (NNNN = next free number).
2. Fill every mandatory section. Delete optional sections that do not apply,
   rather than leaving them empty.
3. Describe WHAT and WHY. Leave HOW to the ADRs — link them instead of
   restating architecture here.
4. Never invent an unstated decision. Write `[NEEDS CLARIFICATION: question]`
   inline and mirror it under "Open questions". A spec with open questions
   is not ready to implement.
5. Every requirement gets a stable ID (FR-001, NFR-001, ...). IDs are
   referenced by tasks, tests and commits — do not renumber them once the
   spec leaves Draft; mark removed ones as `(withdrawn)`.
6. Keep the whole document reviewable in one sitting. If it grows past that,
   the feature is too large — split it.

Remove this comment block in the copy.
-->

## Metadata

- **Status**: Draft <!-- Draft | Review | Accepted | Implemented | Superseded -->
- **Created**: YYYY-MM-DD
- **Updated**: YYYY-MM-DD
- **Related ADRs**: ADR-NNNN, ... <!-- or "none yet" -->
- **Related specs**: SPEC-NNNN (depends on / supersedes / superseded by)

## Summary

<!-- Two or three sentences. What this feature is, for whom, and the change it
makes to the product. A reader should be able to stop here and still know what
is being built. -->

## Problem

<!-- The user-visible or operational problem that motivates the work. Describe
the current behaviour and why it is inadequate. No solution language. -->

## Goals

<!-- Bulleted outcomes this spec commits to. Each one should be verifiable. -->

- ...

## Non-goals

<!-- Explicit scope boundaries. This section is what stops an agent from
gold-plating; be generous with it. -->

- ...

## User scenarios *(mandatory)*

<!-- Order stories by priority (P1 highest). Each story must be independently
implementable and demonstrable: shipping only P1 should still produce something
useful. Acceptance scenarios are Given/When/Then and must be observable from
outside the system. -->

### Story 1 — Brief title (P1)

As a `<user type>`, I want `<capability>`, so that `<benefit>`.

**Acceptance scenarios**

1. **Given** `<initial state>`, **When** `<action>`, **Then** `<observable outcome>`.
2. **Given** `<initial state>`, **When** `<action>`, **Then** `<observable outcome>`.

### Story 2 — Brief title (P2)

...

### Edge cases

<!-- Boundary and failure conditions the stories above do not cover. Each entry
should end up as an FR or an explicit non-goal — an unanswered edge case is an
open question, not a note. -->

- What happens when `<boundary condition>`?
- How does the system behave when `<dependency fails / input is malformed>`?

## Requirements *(mandatory)*

<!-- Write requirements in EARS (Easy Approach to Requirements Syntax). One
behaviour per requirement, testable, no implementation detail:

  Ubiquitous:    THE SYSTEM SHALL <behaviour>
  Event-driven:  WHEN <trigger> THE SYSTEM SHALL <behaviour>
  State-driven:  WHILE <state> THE SYSTEM SHALL <behaviour>
  Unwanted:      IF <condition> THEN THE SYSTEM SHALL <behaviour>
  Optional:      WHERE <feature is present> THE SYSTEM SHALL <behaviour>

Use SHALL for binding requirements, SHOULD for preferences (and say why). -->

### Functional

- **FR-001**: WHEN `<trigger>` THE SYSTEM SHALL `<observable behaviour>`.
- **FR-002**: IF `<error condition>` THEN THE SYSTEM SHALL `<recovery / message / exit code>`.
- **FR-003**: THE SYSTEM SHALL `<invariant>`.

### Non-functional

<!-- Only constraints that are actually binding for this feature: performance
budgets, security properties, compatibility, resource limits, observability.
Give numbers, not adjectives. -->

- **NFR-001**: THE SYSTEM SHALL `<measurable constraint, e.g. add no more than 150 ms to startup>`.
- **NFR-002**: `<security property>`.

## Interface contract

<!-- The surface a user or another program touches. Delete the subsections that
do not apply. This is the part agents get wrong most often when it is implicit —
be exact about names, defaults, and precedence. -->

**CLI**

| Flag / argument | Type | Default | Description |
| --- | --- | --- | --- |
| `--example <VALUE>` | string | none | ... |

**Configuration** *(file keys and matching environment variables)*

| Key | Env var | Type | Default | Description |
| --- | --- | --- | --- | --- |
| `section.key` | `SANDME_SECTION_KEY` | string | `...` | ... |

**Exit codes / errors**

| Code | Condition | Message to user |
| --- | --- | --- |
| `0` | success | — |
| `N` | ... | ... |

## Key entities *(optional)*

<!-- Domain objects the feature introduces or changes: what each represents,
its meaningful fields and relationships. Conceptual, not struct definitions. -->

- **`<Entity>`**: what it represents, key attributes, relationships.

## Constraints and dependencies

<!-- Existing decisions, crates, OS APIs, external services, and prior ADRs that
bind this design. Anything the implementer is not free to change. -->

- ...

## Success criteria *(mandatory)*

<!-- Measurable, implementation-agnostic outcomes that tell you the feature
worked — distinct from requirements, which say what it must do. -->

- **SC-001**: `<metric, e.g. a sandboxed process cannot reach the network except through the proxy, verified by X>`.
- **SC-002**: `<metric>`.

## Verification

<!-- How each requirement will be proven. Every FR/NFR must appear here; a
requirement with no verification either is untestable or does not belong. -->

| Requirement | Verified by |
| --- | --- |
| FR-001 | `<unit / integration test name, or manual procedure>` |
| NFR-001 | `<benchmark, measurement>` |

## Assumptions

<!-- Reasonable defaults chosen where the request was silent. Each one is a
decision the reviewer can overturn cheaply — say it out loud rather than
burying it in the implementation. -->

- ...

## Open questions

<!-- Every [NEEDS CLARIFICATION] marker from above, collected. Must be empty
before Status leaves Draft. -->

- [ ] `<question>` — blocks FR-NNN.

## Implementation tasks

<!-- Ordered, independently reviewable steps, each traceable to requirements.
Keep them coarse enough to be meaningful (a task is a commit or a PR, not a
line edit). Tests come before or with the code that satisfies them. -->

- [ ] **T-001** — `<task>` (covers FR-001, FR-002)
- [ ] **T-002** — `<task>` (covers FR-003)

## Changelog

| Date | Change |
| --- | --- |
| YYYY-MM-DD | Initial draft. |
