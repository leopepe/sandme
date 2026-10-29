## Standards review

**Gate:** fmt ✓ · clippy ✓ 0 warnings · build ✓ 0 warnings · test ✓ 143 tests ran · doc ✓
**Scope:** main working tree, task scope: new `docs/specs/0019-proxy-ip-family-fallback.md`.
The previous task's `src/proxy/server.rs` refactor and report, plus existing agent-tooling,
AGENTS.md, IDE and GEMINI.md changes, are excluded from this proposal's verdict.
The current source gate was run in this turn; it verifies the existing behavior, not
the proposed unimplemented fallback. Tests required unrestricted execution because they
bind loopback sockets and launch sandbox-exec. The one ignored test is the documented
`probe` fixture, not a skipped acceptance test.

**Guidelines applied:** every file in `docs/guidelines/`, listed in the verdict table.

### Verdicts

| Guideline | Rule | Verdict | Finding |
| --- | --- | --- | --- |
| architecture/posix.md | config precedence, streams, exit codes | Pass | Draft specifies env > file > default, stderr-only diagnostics, child status on success and 125 on startup/config failure; no flags added. |
| code/consistency.md | config and error ownership, naming | Pass | Proposal keeps config reading in config, startup in proxy/server, errors in error; uses one setting name throughout. |
| code/quality-gates.md | verification before review | Pass | All commands ran; 77 unit, 63 CLI and 3 tunnel tests passed. No compiler, lint or rustdoc warnings. |
| code/rust.md | Rust source conventions | N/A | This task adds no implementation code. |
| code/simplicity.md | requirement evidence and scope | Pass | User explicitly requested preference plus fallback; draft rules out new dependencies, modules, abstractions and retries. |
| general/handover.md | workflow stage handover | N/A | No agent handoff or context reset; proposal is available for review in this session. |
| general/project-maturity-phases.md | maturity expectations | N/A | No project phase change; file supplies no measurable threshold for this proposal. |
| performance/cli-performance.md | startup cost and measurements | Pass | Draft requires no network probe and no attempt after first success; makes no latency claim and schedules measured performance review for implementation. |
| sdd/spec-driven-development.md | §3.2 proposal, §4 deltas, §5 verification | Pass | Draft follows template, records deltas, defaults and assumptions, has no open questions, and maps every requirement to acceptance scenarios, planned tests and implementation tasks. Baseline annotations await acceptance. |
| tests/testing.md | behavioral proof | Pass | Planned boundary tests use the CLI and controlled loopback fixtures; test rows explicitly distinguish planned tests from existing passing tests. |

### Findings

None within the proposal scope. The proposal does not implement its Draft requirements.

### Traceability

| Changed file | Requirement ID | Verified by | Status |
| --- | --- | --- | --- |
| docs/specs/0019-proxy-ip-family-fallback.md | FR-1901 through FR-1910; NFR-1901 | Acceptance scenarios 1–11, Verification table, T-1901 through T-1905 | Draft; implementation and new tests not due before approval |

### Defects noticed

None in the proposed document. Existing dual-listener behavior is described as the
current baseline, not incorrectly reported as implementing the new requirements.

### Result

Blockers: 0 · Advisories: 0 — proposal is ready for user review. This is a standards
review, not the repository-wide spec-review audit. Required narrower reviews remain
scheduled before a PR; no PR has been opened.
