# AGENTS.md — docs/

Project documentation. Three directories, three different jobs: `specs/` says what to build,
`adrs/` says why a structural decision was made, `guidelines/` says how to work.

## Layout

| Directory | Holds | Naming | Template |
| --- | --- | --- | --- |
| `specs/` | One file per change: requirements, scenarios, tasks, verification | `NNNN-short-feature-name.md` | `0000-spec-template.md` |
| `adrs/` | Architectural decisions and their reasoning | `NNNN-short-adr-name.md` | `0000-adr-template.md` |
| `guidelines/` | Domain rules that apply across changes | `<domain>/<topic>.md` | none |

`NNNN` is zero-padded and sequential; numbers are never reused. Guidelines are grouped by
domain (`architecture/`, `code/`, `sdd/`) rather than numbered — a guideline is found by its subject,
not by when it was written.

**avoid listing files in the guidelines/** Do not list files in the guidelines/ in AGENTS.md or other documentation. this can introduce points of failure and future misleading instructions if files are renamed or added / removed. reference the directories and instruct to read the files in those directories. prefer to refer the directories and the file nameing convention.

## When to read

- **Before proposing anything**: the specs whose `Status` is `Accepted` or `Implemented`. They
  are the current contract.
- **Before changing structure, or a decision expensive to reverse**: `adrs/`. Reading these
  is how you learn why the code looks the way it does. An ADR at `Status: Accepted` is binding;
  `Proposed` ADRs are discussion material and do not authorize implementation. See
  `docs/guidelines/sdd/spec-driven-development.md` §2.1 for the ADR lifecycle and approval gate.

## When to write

- A new capability or a change to observable behaviour gets a spec. The test for whether one
  is needed, and the whole Explore → Propose → Apply → Archive workflow, is in
  `guidelines/sdd/spec-driven-development.md`.
- An architectural decision gets an ADR, linked from the spec. Specs link to ADRs; they never
  restate them.
- A rule that outlives a single change gets a guideline, not a paragraph in a spec.

## Rules

- `guidelines/sdd/spec-driven-development.md` governs specs and ADRs. This file describes the
  layout; that file is the authority on process. Where they appear to disagree, it wins.
- An implemented spec's requirements are never edited in place. Write a new spec with deltas.
- Guidelines are authoritative for their domain. Other documents link to them and MUST NOT
  restate their rules.
