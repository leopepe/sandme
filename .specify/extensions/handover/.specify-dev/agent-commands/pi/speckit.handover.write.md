

<!-- Extension: handover -->
<!-- Config: .specify/extensions/handover/ -->
# /speckit.handover.write

Write a handover document for the current in-flight work.

## When to use

- The context or token limit is approaching and the work is unfinished.
  Write it BEFORE reaching the limit, not at it.
- A workflow exit gate is reached and the next stage runs in a fresh session.
- A scoped task is delegated to another agent.

Do not write one for finished work. The spec, the code and the commits carry that.

## Procedure

1. Determine the current spec (the feature dir under specs/ being worked on).
2. Determine the current stage of the cycle (constitution through archive).
3. Create `specs/<feature-dir>/handover-NNNN.md` where NNNN is zero-padded and
   sequential; numbers are never reused.
4. Fill ALL seven required sections, in this order:

| Section | Contents |
|---------|----------|
| Task | One line: what is being built, and the spec and requirement IDs it serves |
| Status | In progress / Blocked / Complete, plus each implementation task and its state |
| State of the tree | Branch, uncommitted changes, tests currently failing and why |
| Next action | The single next thing to do, concrete enough to start without re-deriving it |
| Decisions | Choices made this session not yet recorded in a spec or ADR, and why |
| Open questions | Anything unresolved that blocks progress, and who must answer it |
| Rejected | Approaches already tried and abandoned, each with its reason |

## Rules

- Cite requirement IDs and path:line. "Fix the config bug" is not a next action.
- MUST NOT restate a guideline, spec or ADR. Link it by path and section.
- A decision that outlives the task MUST be moved into a spec or ADR first.
- State facts, verified. Run the tests before writing what passes.
- A previous handover MUST NOT be edited; write a new one and link the one it continues.

## Picking one up

1. Read the handover in full.
2. Read the spec and ADRs it cites.
3. Confirm the tree matches State of the tree. Run the tests — trust the run, not the file.
4. Continue from Next action.
5. On pausing again, write a new handover that links this one.
