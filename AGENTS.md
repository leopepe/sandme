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

Some changes need a second, narrower review as well. Run it **before opening the PR**, while the
change is still cheap to alter:

| The change touches | Also run | Why this one |
| --- | --- | --- |
| `src/sandbox.rs` (the Seatbelt profile) or `src/proxy.rs` (egress) | `/security-audit` | These two files *are* the sandbox. A profile rule in the wrong order, or a proxy that forwards further than the profile allows, is the product failing while every test stays green. |
| `docs/specs/` — a new spec, a delta, or a `Status` change | `spec-review` agent | Finds specs that contradict each other, requirements nothing implements, and behaviour no requirement covers. |
| Module boundaries — a new module, a moved responsibility, a new trait or layer | `architecture-review` agent | Checks the change against accepted ADRs in `docs/adrs/` and against what each module is for. |
| The startup path, async code, or anything with an `NFR-` budget | `performance-review` agent | Produces measurements. A performance claim with no numbers behind it is not a result. |

Every one of these reports; none of them edits code. Fixing what they find is a separate step.

**Nothing automates this.** There is no CI, no hook and no gate that invokes a review — this table
is the trigger. When you hand work to a subagent, the table goes with the task: an agent that never
reads this file will not run any of them, and "the agent did not know" is not a review having
happened.

<!-- graft:start -->
## Graft — repo context graph

This repo is indexed in `graft/`: small linked markdown nodes that explain each
system and carry exact file:line spans, kept in sync with the code through git.

For ANY task here — understanding how something works, finding where code lives,
or scoping a change — get context from the graph before grepping or opening
source files. Re-ask freely (it's cheap) and reuse literal identifiers you
already have (symbol, error string, file name) as the query. New to this repo?
Run `graft map` first — a token-budgeted orientation (dir clusters, hubs,
hotspots), no LLM, no key.

- Run `graft ask "<your question>" --source` → ranked nodes with the relevant
  code spans inlined (each hit's ≤8-line crux by default; `--full` for whole
  definitions when the crux isn't enough). Match the tool to the task shape:
  for understanding or editing, the top node IS the answer — cite its
  `covers:` file:line spans and edit straight from `--source`. For
  exhaustive tasks ("every occurrence / every caller of this pattern"), ranked
  results are top-N, not complete — run `graft grep "<literal>"` instead
  (exhaustive over indexed files, grouped by enclosing symbol), falling back
  to raw `grep -rn` only for unindexed files.
- `graft skeleton <file>` → every definition's signature + span, ~10× cheaper
  than reading the file; use it to skim an API surface.
- `graft callers <symbol>` gives precomputed, exact edges — who calls this.
  Add `--direction out` for what it calls, or `--depth N` to walk
  transitively for the full blast radius. For structural questions, skip
  ranking and use this directly.
- Or browse: `graft/INDEX.md` lists every node; follow the links.
- Monorepos and folders of multiple repos rank fairly across sub-projects —
  hits carry `[scope/]` labels naming which one they're from. Narrow with
  `graft ask "<task>" --in <scope>/` once you know where you're working.

If a returned span is truncated ("+N more lines"), open the file at that exact
range before finalizing. Only open source files when a node genuinely lacks a
needed detail, and then at the exact file:line the node points to — never
re-read whole files.

After big code changes, refresh the graph with `graft build` (deterministic,
no API key, $0).
<!-- graft:end -->
