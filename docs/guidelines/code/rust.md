# Rust conventions

How Rust source in this repository is shaped, documented and fails.

Audience: humans and code agents. Rules use MUST / MUST NOT / SHOULD.

## 1. Principle

The source is the only description of the code that cannot go stale. Keep it accurate: a module
says what it is for, a public item says why it exists, and an error says what the user should do
about it.

Everything a second document would have to repeat belongs here instead.

## 2. Crate shape

Rust 2024. One binary target, no library target.

Each module is named after its purpose. No index of modules is maintained anywhere outside the
source: the module declarations plus each module's `//!` doc are the map. A list of modules kept
in a separate document is a second source of truth, and it is stale the moment a module is
added.

## 3. Module documentation

- Every module MUST open with a `//!` doc comment stating what the module is for.
- The `//!` doc says what the module is for, not how it works. Implementation detail belongs to
  the items inside it, or to nothing.
- A `//` comment at the top of a file is not module documentation — it does not appear in
  `cargo doc` and does not satisfy this rule.
- If the `//!` doc needs "and" to describe the module, the module has two purposes. See
  `docs/guidelines/code/simplicity.md` §3.

## 4. Rustdoc

- Public items MUST carry rustdoc comments. These generate the published documentation.
- Rustdoc explains **why** the item exists and how to use it.
- A `//` comment inside a body explains a non-obvious decision. Code needing a comment to
  explain **what** it does gets rewritten instead.
- Examples in rustdoc are compiled by `cargo test`. A doc example that does not build is a
  broken test.
- `cargo doc --no-deps` MUST produce no warnings — a broken intra-doc link is a broken
  reference.

## 5. Errors

- One `thiserror` enum in `error.rs` is the crate's error type.
- One variant per distinct failure a caller can act on. Variants a caller cannot distinguish or
  handle differently MUST be merged — see `docs/guidelines/code/simplicity.md` §5.
- Propagate with `?`. A `match` that only re-wraps a `Result` is a `?`.
- Every variant's `#[error("…")]` message is user-facing text. Write it for the person reading
  the terminal, and see `docs/guidelines/architecture/posix.md` §3 for where it is written to.
- MUST NOT `panic!`, `unwrap()` or `expect()` on a condition a user can cause. `expect()` is for
  invariants the code itself guarantees, and its message states the invariant.

## 6. Review checklist

- [ ] Every module opens with a `//!` doc, and it needs no "and".
- [ ] Every public item has rustdoc explaining why it exists.
- [ ] `cargo doc --no-deps` produces no warnings.
- [ ] Error variants are distinguishable by a caller; messages read as user-facing text.
- [ ] Errors propagate with `?`; no `match` only re-wraps a `Result`.
- [ ] No `unwrap()` or `expect()` on user-reachable conditions; every `expect()` states an
      invariant the code guarantees.

## 7. Related

- `docs/guidelines/code/simplicity.md` — size limits, one purpose, over-engineering.
- `docs/guidelines/code/consistency.md` — naming and structure shared across modules.
- `docs/guidelines/code/quality-gates.md` — fmt, clippy, build, test before done and before review.
- `docs/guidelines/tests/testing.md` — where tests live and when they are written.
- `docs/guidelines/architecture/posix.md` — the CLI surface this code presents.
