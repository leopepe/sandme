# AGENTS.md — docs/

Project documentation. Four directories, four different jobs: `specs/` says what to build,
`adrs/` says why a structural decision was made, `guidelines/` says how to work, and `reference/`
says what the shipped tool does for the people using it.

## Layout

| Directory | Holds | Naming | Template |
| --- | --- | --- | --- |
| `specs/` | One file per change: requirements, scenarios, tasks, verification | `NNNN-short-feature-name.md` | `0000-spec-template.md` |
| `adrs/` | Architectural decisions and their reasoning | `NNNN-short-adr-name.md` | `0000-adr-template.md` |
| `guidelines/` | Domain rules that apply across changes | `<domain>/<topic>.md` | none |
| `reference/` | User-facing reference for the shipped behaviour: what the sandbox allows, every config key, exit statuses, known limitations | `<topic>.md` | none |

`NNNN` is zero-padded and sequential; numbers are never reused. Guidelines are grouped by
domain (`architecture/`, `code/`, `sdd/`) rather than numbered — a guideline is found by its subject,
not by when it was written. `reference/` is named by subject for the same reason.

`reference/` is the only directory here addressed to users rather than to contributors. It
describes behaviour that already ships; it does not decide, propose or require anything. A page
there is written after the spec it documents reaches `Implemented`, and links to that spec rather
than restating its requirement text. The README links into `reference/` and keeps only what a
reader needs before deciding to install: what the tool is, how to install and run it, what it
requires, and what it does.

**Never list files.** No document — AGENTS.md or any other — lists the files in `guidelines/`
or in any other directory here. A list of filenames goes stale the moment a file is renamed,
added or removed, and a stale list misdirects the next reader. Name the directory and its naming
convention instead, and instruct the reader to read what the directory holds.

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
- A change to observable behaviour updates the `reference/` page that describes it, in the same
  pull request. A reference page that disagrees with the binary is worse than no page.

## Rules

- `guidelines/sdd/spec-driven-development.md` governs specs and ADRs. This file describes the
  layout; that file is the authority on process. Where they appear to disagree, it wins.
- An implemented spec's requirements are never edited in place. Write a new spec with deltas.
- Guidelines are authoritative for their domain. Other documents link to them and MUST NOT
  restate their rules.
