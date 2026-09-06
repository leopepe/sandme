# AGENTS.md

sandme is a macOS command-line tool that runs a coding IDE or code agent inside a Seatbelt
sandbox, with the sandboxed process's network egress forced through a proxy sandme manages.
Rust 2024, single binary, macOS only.

## Setup commands

- Build: `cargo build`
- Run: `cargo run -- <command>`
- Test: `cargo test`
- Lint: `cargo clippy --all-targets`
- Format: `cargo fmt`
- Docs: `cargo doc --no-deps --open`

## Before implementing

- Confirm an approved spec stands behind the change: a spec in `docs/specs/` covers the
  capability, its `Status` is `Accepted`, its open questions are resolved, and it contradicts
  no existing spec or ADR. Do not write implementation code until that holds.
  `docs/guidelines/sdd/spec-driven-development.md` is the authority on this check, and on the
  §9 fast path that exempts trivial changes from it.

## Before finishing any task

- The quality gate passes — `cargo fmt`, `cargo clippy --all-targets`, `cargo build`, `cargo test`,
  in that order, with no warnings and no failures. Verify by running them, not by asserting it. Make sure all integration tests are passing.
  `docs/guidelines/code/quality-gates.md` is the authority on the gate, and on running it before
  any review is triggered.
- The change traces to a requirement ID in `docs/specs/`, or it qualifies as trivial under the
  fast path in `docs/guidelines/sdd/spec-driven-development.md` §9.

## Dependencies

Add only well-known, well-maintained cargo crates. A new dependency needs a requirement the
standard library and the crates already in `Cargo.toml` cannot satisfy.

## Where to go deeper

Read the nearest AGENTS.md to the file you are editing — it takes precedence over this one.
For domain depth, read every guideline in the matching directory instead of working from
memory:

| Working on | Read next |
| --- | --- |
| Rust source: structure, naming, docs, errors, quality gates | `docs/guidelines/code/` |
| The CLI surface: flags, arguments, streams, exit codes | `docs/guidelines/architecture/` |
| Tests of any kind | `docs/guidelines/tests/` |
| Speed, parallelism, async, profiling | `docs/guidelines/performance/` |
| Planning, specifying or scoping a change | `docs/guidelines/sdd/` |
| Pausing work, or handing a task to another agent | `docs/guidelines/general/` |
| The `docs/` tree itself — layout and what belongs where | `docs/AGENTS.md` |

Directories, not files: guidelines are added over time, and a list of filenames here would go
stale. `docs/AGENTS.md` maps the tree.

The guideline is authoritative for its domain. AGENTS.md files point at it and MUST NOT restate
its rules — a copy is a conflict waiting to happen.

## Reviewing

`/review-standards` audits the current change against every guideline in `docs/guidelines/`.
Run it before proposing a change as complete.
