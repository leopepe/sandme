# Spec-Driven Development

How work is planned, specified, and implemented in this repository. Follows the
[OpenSpec](https://github.com/Fission-AI/OpenSpec) model, adapted to this repo's
`docs/` layout.

Audience: humans and code agents. Rules use MUST / MUST NOT / SHOULD.

## 1. Principle

Agree on **what** to build before writing code. The spec is the contract; the code is
its implementation. When code and spec disagree, one of them is a bug — say which.

The workflow is fluid, not waterfall: any artifact may be revised at any stage. What is
fixed is the **gate** — no implementation without an approved spec.

## 2. Artifact map

OpenSpec's artifacts map onto this repo as follows. Do not create an `openspec/`
directory; use these paths.

| OpenSpec concept | This repo | Notes |
| --- | --- | --- |
| `proposal.md` (why / what changes) | `docs/specs/NNNN-name.md` → Summary, Problem, Goals, Non-goals | One file per change. |
| `specs/` (requirements + scenarios) | same file → User scenarios, Requirements, Interface contract | Requirements carry stable IDs. |
| `design.md` (technical approach) | `docs/adrs/NNNN-name.md` | Only when an architectural decision is made. Specs link to ADRs; they never restate them. |
| `tasks.md` (checklist) | same file → Implementation tasks | Checkboxes, each traceable to requirement IDs. |
| Living capability spec | Any spec with `Status: Implemented` | The accumulated set of implemented specs is the baseline. |
| `changes/` (in-flight) | Any spec with `Status: Draft` or `Review` | Status, not location, marks in-flight work. |
| `archive/` | `Status: Implemented`, plus a Changelog entry | Specs never move or get deleted — IDs are cited by commits, tests and ADRs. |

Templates: `docs/specs/0000-spec-template.md`, `docs/adrs/0000-adr-template.md`. Both are
mandatory shapes, not suggestions.

## 3. Workflow

Four stages. Each has an exit gate that MUST hold before advancing.

### 3.1 Explore

**Trigger:** the request is vague, or the approach is unclear.

- Read `docs/specs/` (existing requirements and their status) and `docs/adrs/`
  (decisions already binding) before proposing anything.
- Read the relevant source under `src/` to ground the proposal in what exists.
- Weigh options with the user. Do not write files during this stage.

**Exit gate:** the user has chosen an approach and the scope is nameable in one line.

Skip this stage only when the user states what they want with enough precision to write
requirements directly.

### 3.2 Propose

**Trigger:** approach agreed, or the request was already precise.

1. Pick the next free `NNNN` in `docs/specs/` (zero-padded, sequential).
2. Copy `docs/specs/0000-spec-template.md` to `docs/specs/NNNN-short-feature-name.md`.
   Delete the instruction comment block.
3. Fill every mandatory section. Delete optional sections that do not apply; never
   leave a section empty.
4. Set `Status: Draft`.
5. If the change alters architecture, cross-cutting structure, or a decision that is
   expensive to reverse, write an ADR and link it under `Related ADRs`.

**Rules:**

- Describe WHAT and WHY. HOW belongs in the ADR or the implementation.
- MUST NOT invent an unstated decision. Where the request is silent and the answer
  matters, write `[NEEDS CLARIFICATION: question]` inline and mirror it under
  **Open questions**.
- Where the request is silent and a reasonable default exists, take the default and
  record it under **Assumptions** — an assumption is cheap to overturn, a hidden
  choice is not.
- Size: one spec = one coherent capability, reviewable in one sitting. If it grows past
  that, split it into multiple specs and link them under `Related specs`.

**Exit gate:** the spec is written and presented to the user for review. Stop here.
Do not implement.

### 3.3 Apply

**Trigger:** the user approves the spec.

1. Resolve every open question. **Open questions MUST be empty before `Status` leaves
   `Draft`.** If one cannot be resolved, either ask, or convert it into an explicit
   non-goal.
2. Set `Status: Accepted`.
3. Work the **Implementation tasks** list in order. For each task:
   - Write or update tests first, or alongside the code — never after.
   - Implement.
   - Verify against the requirement IDs the task covers.
   - Tick the checkbox in the spec file.
4. Commit per task, not per file. Reference the requirement IDs in the message
   (e.g. `feat(config): load ~/.sandme/config.toml (FR-007, FR-008)`).

**If reality contradicts the spec mid-implementation** — a requirement is infeasible,
an interface is wrong, an edge case was missed — stop, update the spec, and say what
changed. MUST NOT silently implement something other than what the spec says.

**Exit gate:** all tasks ticked, all rows in the **Verification** table satisfied by a
test that exists and passes.

### 3.4 Archive

**Trigger:** implementation complete and verified.

1. Set `Status: Implemented`, update `Updated`.
2. Add a **Changelog** row stating what landed.
3. The spec stays where it is. It is now part of the baseline.

## 4. Changing an implemented spec

Never edit the requirements of an `Implemented` spec in place — its IDs are referenced
by tests and commits. Instead, write a new spec that declares deltas against it, using
OpenSpec's ADDED / MODIFIED / REMOVED semantics.

In the new spec:

- `Related specs`: `SPEC-000X (supersedes)` or `SPEC-000X (extends)`.
- Add a **Spec deltas** section immediately after Summary:

```markdown
## Spec deltas

### ADDED
- **FR-101**: WHEN <trigger> THE SYSTEM SHALL <behaviour>.

### MODIFIED
- **SPEC-0001/FR-006** — was: <old text>. Now: <new text>. Reason: <why>.

### REMOVED
- **SPEC-0001/FR-009** (withdrawn) — reason: <why>.
```

- In the superseded spec, mark the affected requirements `(withdrawn)` or
  `(superseded by SPEC-000Y/FR-NNN)` and add a Changelog row. Change its `Status` to
  `Superseded` only if the whole spec is replaced.

Requirement IDs are never reused and never renumbered.

## 5. Writing requirements

Use EARS. One behaviour per requirement, testable, no implementation detail.

```
Ubiquitous:    THE SYSTEM SHALL <behaviour>
Event-driven:  WHEN <trigger> THE SYSTEM SHALL <behaviour>
State-driven:  WHILE <state> THE SYSTEM SHALL <behaviour>
Unwanted:      IF <condition> THEN THE SYSTEM SHALL <behaviour>
Optional:      WHERE <feature is present> THE SYSTEM SHALL <behaviour>
```

- `SHALL` = binding. `SHOULD` = preference, and MUST state why.
- Functional requirements are `FR-NNN`; non-functional are `NFR-NNN`.
- Non-functional requirements MUST give numbers, not adjectives. "fast" is not a
  requirement; "adds no more than 150 ms to startup" is.
- Every requirement MUST be covered by at least one acceptance scenario and MUST appear
  in the Verification table. A requirement with no verification is either untestable or
  does not belong.

Acceptance scenarios are Given/When/Then and MUST be observable from outside the
system — an assertion about internal state is a unit test, not a scenario.

```markdown
1. **Given** <initial state>, **When** <action>, **Then** <observable outcome>.
```

Stories are prioritised P1..Pn and MUST be independently demonstrable: shipping only P1
still produces something useful.

## 6. Traceability

The chain MUST be unbroken in both directions:

```
Story → FR/NFR → Acceptance scenario → Verification row → Test → Task → Commit
```

Given any requirement ID you must be able to find the test that proves it; given any
test you must be able to name the requirement it serves.

## 7. Validation checklists

**Before requesting approval (end of Propose):**

- [ ] Every mandatory section filled; no empty sections left behind.
- [ ] Every `[NEEDS CLARIFICATION]` marker mirrored under Open questions.
- [ ] Every FR/NFR is testable and free of implementation detail.
- [ ] Non-goals are explicit and generous.
- [ ] Verification table covers every FR and NFR.
- [ ] Implementation tasks each cite the requirements they cover.

**Before implementing (start of Apply):**

- [ ] `Status: Accepted`, Open questions empty.
- [ ] Related ADRs written for any architectural decision.
- [ ] No requirement in the spec is contradicted by an existing Accepted/Implemented spec.

**Before archiving:**

- [ ] All tasks ticked.
- [ ] All verification tests exist and pass — verified by running them, not by assertion.
- [ ] Spec text matches what was actually built.
- [ ] Changelog row added; `Status: Implemented`.

## 8. Prohibitions

- MUST NOT write implementation code for a change that has no spec at `Accepted`.
- MUST NOT resolve an ambiguity by guessing. Ask, or record it as an assumption or open
  question.
- MUST NOT implement anything outside the spec's Goals — Non-goals exist to stop
  gold-plating.
- MUST NOT restate architecture in a spec. Link the ADR.
- MUST NOT delete a spec, renumber a requirement, or reuse a requirement ID.
- MUST NOT mark a task or spec complete without having run the verification.

## 9. Fast path

Trivial work — typo fixes, dependency bumps, comment edits, formatting, a rename with
no behavioural change — does not need a spec. The test: if it changes observable
behaviour, an interface, or a stated requirement, it needs a spec. If in doubt, it
needs a spec.

## 10. Quick reference

| Situation | Action |
| --- | --- |
| Vague request | Explore (§3.1). No files written. |
| Clear new capability | Propose new `docs/specs/NNNN-*.md` (§3.2), then stop. |
| Change to implemented behaviour | New spec with **Spec deltas** (§4). |
| Architectural decision needed | ADR in `docs/adrs/`, linked from the spec (§2). |
| Spec approved | Apply: resolve questions → `Accepted` → tasks in order (§3.3). |
| Spec turns out wrong mid-build | Stop, update the spec, report the change (§3.3). |
| Implementation done and verified | Archive: `Implemented` + Changelog (§3.4). |
| Trivial change | Fast path (§9). |
| Context running out mid-implementation | Write a handover (`docs/guidelines/general/handover.md`). |

## 11. Related

- `docs/AGENTS.md` — the `docs/` layout these artifacts are filed into.
- `docs/guidelines/code/simplicity.md` — requirements are the evidence that justifies code.
- `docs/guidelines/tests/testing.md` — how the Verification table is satisfied.
- `docs/guidelines/general/handover.md` — pausing work between stages or sessions.
