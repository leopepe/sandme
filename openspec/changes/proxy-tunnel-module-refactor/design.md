## Context

See `proposal.md` for motivation. Accepted SPEC-0018 (`docs/specs/0018-proxy-and-tunnel-module-layout.md`) is the requirement/verification authority; accepted ADR-0001 (`docs/adrs/0001-proxy-and-tunnel-module-boundaries.md`) is the already-approved structural decision. The original Type-B evidence is `.agents/handovers/0002-conductor-proxy-tunnel-implementation.md`; owner approval is recorded in `.conductor/proxy-tunnel-module-refactor/approval.md`. Historical amendment planning packet 004 and `.conductor/proxy-tunnel-module-refactor/scope-amendment-001.md` record the owner's pointer-only T-1804 amendment; the current planning packet is 005. The historical full Rust gate is RED from the seven named lint findings, so implementation reviews await T-1801's clean gate.

## Goals / Non-Goals

**Goals:** Follow ADR-0001 D1–D6 and SPEC-0018's four canonical tasks; keep crate-facing paths, behavior and existing security coverage intact across the source move on macOS and Linux.

**Non-Goals:** This working design makes no new architectural choice and defines no second requirements baseline. It does not authorize fixes outside T-1801's named findings, new platform behavior, sandbox edits or replacement verification.

## Decisions

- **Boundary and visibility:** Use ADR-0001 D1–D4 as written: sibling components, declaration/re-export-only facades, request-local security chain and shared root credential ownership. ADR-0001 records the rejected nesting, filled-facade and new-layer alternatives. Implementation does not reopen them.
- **Tests and active traceability:** Follow ADR-0001 D5–D6 and SPEC-0018 T-1802–T-1804. Binary-driven listener and SSH failure characterizations precede deletion/relocation of old cases; historical narrative and older requirement/status text are not rewritten. T-1804 also updates the proxy/tunnel review pointers in `CONTRIBUTING.md` and `.github/pull_request_template.md` only, preserving their other protected paths. Test execution still does not replace the mandated source/security review.
- **Verification ownership:** Single writer executes T-1801 → T-1802 → T-1803 → T-1804 in the current worktree. Run task-specific characterizations at the prescribed pre-relocation points in T-1802 step 2 and T-1803 steps 1–2 (before step 3 extraction), and selected post-relocation tests at their prescribed points. After each Rust task, run and read `cargo fmt`, `cargo fmt --check`, `cargo clippy --all-targets`, `cargo build`, `cargo test` in order, plus warning-free `cargo doc --no-deps`; only after a clean gate commit and request implementation review. Workers do not edit canonical task/status bookkeeping independently.

## Risks / Trade-offs

- [Baseline RED or a newly exposed defect] → T-1801 repairs only the seven documented findings; any additional lint class, failing test or behavior change stops for disposition rather than growing the refactor.
- [Relocated security-sensitive paths missed by legacy trigger matching] → T-1804 names both moved subtrees explicitly in the security and repository reviews, retaining old protected-path coverage; a green test run does not replace this inspection.
- [Linux kernel enforcement unverified] → require execution on kernel 6.7+ with Landlock ABI v4 and exact retained evidence before T-1804/cycle completion; macOS results or cross-compilation do not close that gap. No compatible runner is yet established.

## Migration Plan

Follow the sequence and exact scopes in SPEC-0018 T-1801–T-1804. Before each relocation, run the named characterization against the still-flat implementation; after each Rust task, collect the full gate and evidence before its local commit/review. T-1804 compares the baseline and relocated test inventory and timing, checks active pointers and reports, then awaits Linux evidence. There is no deployed data or user-facing migration; do not mark canonical implementation or archive complete while required checks remain open.
