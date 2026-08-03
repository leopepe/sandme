# Consistency

How new code is made to match the code already here, and how a second copy of an existing
behaviour is avoided.

Audience: humans and code agents. Rules use MUST / MUST NOT / SHOULD.

## 1. Principle

Code written today looks like the code written yesterday. A reader who has understood one module
can predict the shape of the next.

Consistency outranks individual preference. Where this repository has already settled a pattern,
follow it — a better pattern applied to one file out of five makes the codebase worse, not
better. Change the pattern everywhere, or not at all.

## 2. Reuse before writing

- Before writing a function, type or module, search `src/` for one that already does the job.
  Search by the domain noun and verb (`rg 'proxy_port'`, `rg 'fn load'`), not by the name you
  were about to invent.
- It already exists → call it.
- It nearly exists → refactor it to cover both callers. Refactoring for reuse is preferred over
  writing a near-duplicate.
- It does not exist → write it, in the module whose stated purpose already covers it, per
  `docs/guidelines/code/simplicity.md` §3.
- Two implementations of one behaviour are a Blocker. One of them gets maintained; the other
  keeps the bug.
- Reuse is bounded by the rule of three in `docs/guidelines/code/simplicity.md` §5. Reuse what
  exists; do not invent an abstraction to serve a second caller.

## 3. Naming

- One name per concept, crate-wide. `proxy_port` is `proxy_port` everywhere — never also `port`,
  `listen_port` or `p`.
- One verb per action, crate-wide. `load` reads from outside the process, `build` constructs in
  memory, `run` executes. MUST NOT introduce a synonym for an established verb.
- Follow `rustfmt` and standard Rust casing: `snake_case` items and modules, `CamelCase` types,
  `SCREAMING_SNAKE_CASE` constants. `cargo fmt` decides layout; do not argue with it.
- A module is named for its purpose, as a singular noun: `config`, `proxy`, `sandbox`, `error`.
  A new module MUST match that pattern.
- Name after the domain, not the shape — see `docs/guidelines/code/simplicity.md` §6.

## 4. Structural consistency

- A new module MUST follow the shape of its siblings: `//!` doc, imports, public items, then
  `#[cfg(test)] mod tests`.
- Errors go in the one `thiserror` enum in `error.rs` — see `docs/guidelines/code/rust.md` §5. A
  module MUST NOT define a private error type as an exception.
- Configuration is read in `config.rs` and passed in. A module MUST NOT read the environment or a
  config file for itself.
- Where a sibling module already solves a problem (argument validation, path expansion, error
  wrapping), solve it the same way or change both.

## 5. Consistency with the spec

- Implemented behaviour MUST match the accepted spec. Where the spec names a thing, the code
  SHOULD use the same noun, so a reader can move between them without a glossary.
- Behaviour outside the spec's Goals MUST NOT be implemented — see
  `docs/guidelines/sdd/spec-driven-development.md` §8.
- Where code and spec disagree, one is a bug. Stop, say which, and fix that one — see
  `docs/guidelines/sdd/spec-driven-development.md` §3.3.

## 6. Review checklist

- [ ] `src/` was searched for an existing implementation before the new one was written.
- [ ] No two functions implement the same behaviour.
- [ ] A near-duplicate was refactored into one function rather than forked.
- [ ] Every concept has one name, and every action one verb, across the crate.
- [ ] Module names are singular purpose nouns matching their siblings.
- [ ] `cargo fmt` produces no diff.
- [ ] New errors are variants of the `error.rs` enum, not a local type.
- [ ] Names in the code match the nouns used in the spec.

## 7. Related

- `docs/guidelines/code/simplicity.md` — one purpose per unit, rule of three, domain naming.
- `docs/guidelines/code/rust.md` — module docs, rustdoc, the crate error type.
- `docs/guidelines/code/quality-gates.md` — `cargo fmt` decides layout; run it before review.
- `docs/guidelines/sdd/spec-driven-development.md` — the spec the code must agree with.
