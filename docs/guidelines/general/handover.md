# Handover

How a code agent pauses unfinished work and hands it to the next agent — itself after a context
reset, or a different agent picking up the next stage.

Audience: humans and code agents. Rules use MUST / MUST NOT / SHOULD.

## 1. Principle

A handover document replaces the context that is about to be lost. The next agent starts with the
repository and this file, and no memory of the session that wrote it. Anything the file does not
say is gone.

Write it for a reader who knows the repository and knows nothing about the session.

## 2. When to write one

Write a handover when:

- The context or token limit is approaching and the work is unfinished. Write it **before**
  reaching the limit, not at it — a handover that runs out of context halfway through is worse
  than none, because it looks complete.
- A workflow exit gate is reached and the next stage will run in a fresh session — see
  `docs/guidelines/sdd/spec-driven-development.md` §3.
- A scoped task is delegated to another agent that MUST NOT inherit this session's context.

Do not write one for finished work. The spec, the code and the commits carry that.

## 3. Location and naming

- Handovers live in `.agents/handovers/`, named `NNNN-short-task-name.md`. The directory is not
  in the repository until the first handover is written — create it then. `NNNN` is zero-padded
  and sequential; numbers are never reused.
- One file per handover. A previous handover MUST NOT be edited; write a new one and link the
  one it continues.
- Handovers are kept, not deleted. Set `Status: Complete` when the work lands.

## 4. Required contents

Every handover MUST contain these sections, in this order.

| Section | Contents |
| --- | --- |
| Task | One line: what is being built, and the spec and requirement IDs it serves |
| Status | `In progress` / `Blocked` / `Complete`, plus each implementation task and its state |
| State of the tree | Branch, uncommitted changes, tests currently failing and why |
| Next action | The single next thing to do, concrete enough to start without re-deriving it |
| Decisions | Choices made this session that are not yet recorded in a spec or ADR, and why |
| Open questions | Anything unresolved that blocks progress, and who must answer it |
| Rejected | Approaches already tried and abandoned, each with its reason |

`Rejected` is what stops the next agent repeating the session that just ended.

## 5. Rules

- Cite requirement IDs and `path:line`. "Fix the config bug" is not a next action;
  "`src/config.rs:42` — env vars override CLI flags, reversing the precedence FR-007 requires" is.
- MUST NOT restate a guideline, spec or ADR. Link it by path and section.
- MUST NOT record what the repository already records. Git says what changed; the spec says what
  is required; the guideline says how to work.
- A decision that outlives the task MUST be moved into a spec, ADR or guideline before the
  handover is written. The handover is a baton, not an archive.
- State facts, verified. Run the tests before writing what passes — see `AGENTS.md`.
- Keep it short enough to read in full. A handover longer than the spec is a symptom that
  something belongs in the spec.

## 6. Picking one up

1. Read the handover in full.
2. Read the spec and ADRs it cites, then every guideline in the directory matching the work.
3. Confirm the tree matches **State of the tree**. Run the tests — trust the run, not the file.
4. Continue from **Next action**.
5. On pausing again, write a new handover that links this one. Do not edit it.

## 7. Review checklist

- [ ] Written before the context limit, not at it.
- [ ] Filed at `.agents/handovers/NNNN-short-task-name.md` with a fresh number.
- [ ] All seven required sections present and non-empty.
- [ ] Next action names a file, a line and a requirement ID.
- [ ] Test and branch state was verified by running, not asserted.
- [ ] No guideline, spec or ADR restated — links only.
- [ ] Durable decisions moved into a spec, ADR or guideline first.
- [ ] Rejected approaches recorded with reasons.

## 8. Related

- `docs/guidelines/sdd/spec-driven-development.md` — the stages and exit gates a handover sits
  between.
- `docs/AGENTS.md` — what belongs in `specs/`, `adrs/` and `guidelines/` instead.
