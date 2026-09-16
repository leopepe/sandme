## What changed and why

<!-- The behaviour before, the behaviour after, and the reason for the difference. -->

## Traceability

<!-- The requirement ID this implements, e.g. FR-0012 / NFR-0003 — or say why the change is
     trivial under the fast path in docs/guidelines/sdd/spec-driven-development.md §9. -->

- [ ] Traces to `FR-`/`NFR-`: <!-- ID --> — or trivial under the §9 fast path because: <!-- reason -->

## Quality gate

Run in this order, with no warnings and no failures (AGENTS.md, "Before finishing any task"):

- [ ] `cargo fmt`
- [ ] `cargo clippy --all-targets`
- [ ] `cargo build`
- [ ] `cargo test`

## Narrower review

AGENTS.md requires a second review for some changes, run before the PR is opened. Tick the
one that applies, or the last box if none does.

- [ ] `/security-audit` — touches `src/profile.rs`, `src/proxy.rs`, `src/egress.rs` or `src/sandbox.rs`
- [ ] `spec-review` — adds or changes something in `docs/specs/`
- [ ] `architecture-review` — new module, moved responsibility, new trait or layer
- [ ] `performance-review` — startup path, async code, or an `NFR-` budget
- [ ] None of these apply
