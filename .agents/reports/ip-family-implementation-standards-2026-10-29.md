## Standards review

**Gate:** fmt ✓ · clippy ✓ 0 warnings · build ✓ 0 warnings · test ✓ 150 tests ran · doc ✓
**Scope:** main working tree relative to HEAD, task files in `src/`, `tests/`,
`docs/specs/`, `docs/reference/` and `examples/config.toml`, including untracked SPEC-0019.
Pre-existing `.claude/`, AGENTS.md, `.idea/`, GEMINI.md changes and previous review reports
are outside this task verdict. No PR has been opened.
**Guidelines applied:** all ten files in `docs/guidelines/`, listed below.

The gate ran after the last source edit. Unit tests: 81; CLI integration tests: 66;
tunnel integration tests: 3. The ignored `probe` is a documented fixture driven by
other tests. Integration execution used loopback/sandbox-exec permissions; no assertions
were suppressed. macOS was exercised; Linux runtime remains unverified.

### Verdicts

| Guideline | Rule | Verdict | Finding |
| --- | --- | --- | --- |
| architecture/posix.md | precedence, streams, exit codes | Pass | New preference uses environment > file > default. Fallback warns on stderr, both failures and invalid input exit 125, and child streams remain inherited. |
| code/consistency.md | naming, reuse, config/error ownership | Pass | Both families use the same setup and accept loop; typed preference is read only in config and errors stay in SandmeError. |
| code/quality-gates.md | execute and inspect gates | Pass | fmt/check, clippy, build, tests and doc passed; no compiler, lint or rustdoc warnings. |
| code/rust.md | module/rustdoc, error propagation, user failures | Pass | New public enum and variants documented; explicit matches implement fallback or dispatch, not mere Result rewrapping; user input cannot panic. |
| code/simplicity.md | purpose, evidence, limits, dependencies | Pass | Approved FR-1901 through FR-1910 justify new code; no dependency/layer added and clippy limits pass. Config's existing file-size escape rationale now cites SPEC-0019 and avoids stale line counts. Large CLI-test file is pre-existing. |
| general/handover.md | unfinished work or delegation without context | N/A | Review agents inherit task context; no implementation handoff or context reset. |
| general/project-maturity-phases.md | maturity expectations | N/A | Task changes no project phase; guideline supplies no measurable threshold here. |
| performance/cli-performance.md | startup, concurrency, output | Pass | Only required bind/setup precedes child spawn. No preflight network probe, new buffering or parallel primitive. Required performance agent measured release startup in both preferences and fallback directions. |
| sdd/spec-driven-development.md | approval, deltas, traceability, verification | Pass | User approved SPEC-0019 before implementation; status Accepted recorded before code. Baseline clauses annotated without rewriting requirements, and verification names map to passing tests. Linux gap is explicit. |
| tests/testing.md | observable assertions, boundary placement, variation classes | Pass | Real CLI tests cover both family choices/fallbacks, total failure, invalid strings/types/empty values, all proxy variables, lifetime and disabled proxy; parser/merge tests exercise config logic. Existing policy tests retain their assertions for selected IPv6; HTTP and SSH relay tests now exercise both preferences. |

### Findings

None in the requested change against the guidelines. Security findings and their
reproduction details are retained privately under SECURITY.md. This public standards
verdict is not a security approval; maintainer security disposition remains required.

### Traceability

| Changed file | Requirement ID | Verified by | Status |
| --- | --- | --- | --- |
| src/config.rs | FR-1901, FR-1909 | `defaults_proxy_ip_version_to_ipv4`, `loads_proxy_ip_version_from_file`, `environment_overrides_proxy_ip_version`, `rejects_invalid_proxy_ip_version` | Covered |
| src/error.rs | FR-1906, FR-1909 | `reports_a_proxy_that_cannot_bind_with_the_reserved_status`, CLI `rejects_invalid_proxy_ip_version` | Covered |
| src/main.rs | FR-1909 | CLI `rejects_invalid_proxy_ip_version` asserts status 125 and no child marker | Covered |
| src/proxy/server.rs | FR-1902 through FR-1907, FR-1910 | Both `falls_back_to_*` tests, `publishes_the_selected_ephemeral_proxy_address`, `uses_the_preferred_loopback_family`, startup failure and lifetime tests | Covered |
| src/sandbox/seatbelt/profile.rs | FR-1901 (test-constructor update only) | Existing profile unit tests | Covered; no production policy change |
| src/sandbox/landlock/plan.rs | FR-1901 (test-constructor update only) | Existing plan unit tests | Covered on host; Linux enforcement unrun |
| tests/cli.rs | FR-1901 through FR-1910, NFR-1901 | 66 passing CLI tests; both preferences exercise HTTP/SSH and proxy-disabled behavior | Covered on macOS |
| docs/specs/0019-proxy-ip-family-fallback.md | FR-1901 through FR-1910, NFR-1901 | Verification table, tasks and final validation evidence | Implemented; all reviews completed, Linux gap recorded |
| docs/specs/0011-proxy-hardening-follow-ups.md | Superseded FR-1105 | New fallback tests replace old optional-secondary warning characterization | Covered |
| docs/specs/0014-undocumented-shipped-behaviour.md | FR-1404 extended by FR-1904 | `publishes_the_selected_ephemeral_proxy_address` | Covered |
| docs/specs/0018-proxy-and-tunnel-module-layout.md | NFR-1801 dual-listener clause superseded | Family-selection tests replace old simultaneous-listener characterization | Covered |
| docs/reference/configuration.md | FR-1901, FR-1902, FR-1905, FR-1906, FR-1909, FR-1910 | Table/example checked against accepted spec and source | Covered |
| examples/config.toml | FR-1901 | Exact key, default, allowed values and fallback checked against source | Covered |

### Defects noticed

Security details are withheld from this public copy under SECURITY.md. The original
report is preserved privately; see the public security-review summary for disposition.

### Result

Blockers: 0 · Advisories: 0 — implementation meets the scoped standards checks, not a
security merge approval. Private security disposition is pending, and this report makes
no Linux enforcement claim.
