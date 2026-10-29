## Spec review — SPEC-0019 (public summary)

This is a publication-safe summary of the SPEC-0019 conclusions from the original
repository-wide review of 2026-10-29, not a new independent review or a clean bill
for the entire repository. The full original report is preserved privately under
SECURITY.md because it also contains security-relevant observations.

**Gate recorded in the original review:** fmt/check, Clippy, build, 150 passing
tests (81 unit, 66 CLI, 3 tunnel; one documented ignored fixture), and docs, with
no compiler/lint/rustdoc warnings. macOS was exercised; Linux was not.

**Scope:** SPEC-0019's 11 requirements and its supersession annotations. The original
review assessed the approved Accepted spec; the implementation subsequently set it
to Implemented. Publication sanitization does not change requirements or tests.

### Coverage

| Requirement | Verified by |
| --- | --- |
| FR-1901 | Config default/file/environment tests; `uses_the_preferred_loopback_family` |
| FR-1902 | `falls_back_to_ipv6_when_ipv4_is_unavailable`, `falls_back_to_ipv4_when_ipv6_is_unavailable` |
| FR-1903 | Fixed-port fallback tests; `publishes_the_selected_ephemeral_proxy_address` |
| FR-1904 | `publishes_the_selected_ephemeral_proxy_address`; authenticated relay and SSH tests |
| FR-1905 | Fallback diagnostics in both fallback tests; preferred-success silence |
| FR-1906 | `reports_a_proxy_that_cannot_bind_with_the_reserved_status` |
| FR-1907 | `proxy_lifetime_follows_command` for both preferences |
| FR-1908 | `proxy_disabled_wires_no_proxy_and_denies_egress` for both preferences |
| FR-1909 | Config and CLI `rejects_invalid_proxy_ip_version` |
| FR-1910 | `uses_the_preferred_loopback_family` |
| NFR-1901 | Credential refusal, restricted-destination, authenticated HTTP and SSH tests for both preferences; unchanged Cargo dependencies |

The original review found no new SPEC-0019 contradiction, duplicate ID, orphan
delta or broken supersession. SPEC-0011/FR-1105 and SPEC-0018's dual-listener clauses
are annotated as superseded; SPEC-0014/FR-1404 is extended without rewriting the
original requirement text.

### Limits and disposition

- The original repository-wide review recorded pre-existing lifecycle, measurement
  and verification gaps. Those remain unresolved; this scoped public summary does
  not replace or erase the private original or certify repository-wide compliance.
- Linux execution is unverified, not a passing result.
- Security findings are retained privately. Maintainer disposition is required
  before merge; passing functional coverage is not security approval.

### Result

All 11 SPEC-0019 requirements had passing named macOS evidence in the original
review. Publication redaction changes neither that functional evidence nor the
outstanding platform and private-review limitations.
