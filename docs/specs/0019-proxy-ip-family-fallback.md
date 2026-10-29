# SPEC-0019: Preferred proxy IP family with automatic fallback

## Metadata

- **Status**: Implemented
- **Created**: 2026-10-29
- **Updated**: 2026-10-29
- **Related ADRs**: ADR-0001 (existing module ownership remains applicable)
- **Related specs**: SPEC-0011 (supersedes FR-1105 and its diagnostic),
  SPEC-0018 (qualifies NFR-1801's dual-listener runtime preservation),
  SPEC-0001 (extends configuration), SPEC-0003 and SPEC-0012 (preserves authentication,
  egress and SSH transport), SPEC-0004 (preserves status 125), SPEC-0014 (extends FR-1404),
  SPEC-0017 (preserves current source defaults and proxy-disabled behavior; remains Draft)

## Summary

Let users choose a preferred proxy IP family, defaulting to IPv4. Start one loopback
listener in that family, automatically trying the other family when setup fails.
Publish the address that actually succeeded so the sandboxed command uses the working listener.

## Spec deltas

These deltas were approved on 2026-10-29. Supersession annotations in the affected
baseline specs preserve their original requirement text.

### ADDED

- FR-1901 through FR-1910 and NFR-1901 below.

### MODIFIED

- **SPEC-0011/FR-1105 and Interface contract — IPv6 bind diagnostic** — was: start an
  optional IPv6 listener alongside mandatory IPv4, warning if IPv6 cannot bind and
  continuing on IPv4. Replacement: select a preferred family, try its opposite
  on setup failure, and warn only when that fallback succeeds (FR-1902, FR-1905).
  If neither succeeds, fail startup (FR-1906). Do not attempt IPv6 after successful
  preferred IPv4 setup. This replaces the old diagnostic, whose word "also" describes
  the previous two-listener behavior.
- **SPEC-0018/NFR-1801, Story 2 scenario 3 and Interface contract — dual-loopback
  preservation** — was: preserve both IPv4 and IPv6 listeners on the published port.
  Replacement: one selected loopback listener, with fallback, as specified here.
  Other preservation guarantees of SPEC-0018 remain. Its historical relocation tasks
  stay historical; the associated dual-listener characterization is replaced by the
  family-selection scenarios below.
- **SPEC-0014/FR-1404** — retains actual ephemeral-port publication; additionally publish
  the literal IP address of the listener that succeeded (FR-1904), including an IPv6
  address when selected or reached by fallback.

## Problem

The proxy requires IPv4 startup to succeed before it attempts IPv6. A system with
working IPv6 loopback but unavailable IPv4 loopback therefore cannot start the proxy.
Users also cannot express an IPv6 preference. The two-listener lifetime does not reflect
the requested behavior of choosing one usable family before running the command.

## Goals

- Default to IPv4 while allowing an IPv6 preference through configuration.
- Recover from failure of either preferred family by trying the other once.
- Expose one working listener address consistently to the child and sandbox.
- Make fallback and total startup failure visible without exposing credentials.

## Non-goals

- Simultaneous IPv4 and IPv6 listeners, wildcard binds, arbitrary bind addresses or remote access.
- Strict family selection without fallback, runtime listener failover, or retry loops.
- Discovering interfaces, testing internet connectivity, or selecting outbound destination families.
- New CLI flags, Cargo dependencies, modules, transport abstractions or performance budgets.
- Changing authentication, destination restrictions, forwarding, SSH protocol or sandbox grants.
- Reconciling unrelated legacy Draft spec statuses or changing existing config-key parsing policies.
- Preserving clients that ignore the published literal proxy address and choose the other family.

## User scenarios

### Story 1 — Use the preferred available family (P1)

As a user, I want IPv4 by default or IPv6 when configured, so my command uses my preferred loopback family.

**Acceptance scenarios**

1. **Given** both families are available and no family setting is supplied, **When** sandme
   starts, **Then** it publishes an IPv4 proxy URL without a fallback warning (FR-1901, FR-1902).
2. **Given** `proxy_ip_version = "ipv6"` and working IPv6 loopback, **When** sandme starts,
   **Then** it publishes a bracketed IPv6 proxy URL without a fallback warning (FR-1901, FR-1904).
3. **Given** conflicting file and environment family settings, **When** configuration is loaded,
   **Then** the environment preference wins (FR-1901).
4. **Given** an invalid family setting in either source, **When** sandme starts, **Then** it
   exits 125 with a config diagnostic and does not launch the command (FR-1909).

### Story 2 — Recover when the preferred family cannot listen (P1)

As a user, I want the other family tried automatically, so one unavailable loopback family
does not prevent my command from running.

**Acceptance scenarios**

5. **Given** preferred IPv4 is occupied on a configured nonzero port and IPv6 is free,
   **When** sandme starts, **Then** it runs through IPv6 on that port and warns about the
   failed IPv4 attempt (FR-1902 through FR-1905).
6. **Given** preferred IPv6 is occupied on a configured nonzero port and IPv4 is free,
   **When** sandme starts, **Then** it runs through IPv4 on that port and warns about the
   failed IPv6 attempt (FR-1902 through FR-1905).
7. **Given** both loopback addresses are occupied on the configured port, **When** sandme
   starts, **Then** it exits 125, reports both failures and never runs the command (FR-1906).

### Story 3 — Preserve proxy integration after selection (P1)

As a user, I want the selected listener to support the existing proxy features and lifetime.

**Acceptance scenarios**

8. **Given** either selected family and port zero, **When** the command uses its injected
   proxy URL, **Then** it reaches the listener on its actual nonzero port; authentication,
   HTTP forwarding, restricted-destination rejection and SSH CONNECT work as before
   (FR-1903, FR-1904, NFR-1901).
9. **Given** a running command, **When** it exits, **Then** its listener closes (FR-1907).
10. **Given** the proxy is disabled, **When** sandme starts, **Then** neither family is
    bound and no fallback diagnostic is emitted (FR-1908).
11. **Given** the preferred family is available on a fixed port, **When** sandme starts,
    **Then** only that family is served by sandme on the port (FR-1910).

### Edge cases

- An unsupported family or address-in-use error triggers the same bounded fallback (FR-1902).
- Port zero applies independently to each attempt; publish only the successful port (FR-1903, FR-1904).
- Failure preparing a socket for acceptance counts as a setup failure; release it before fallback (FR-1902).
- A client choosing an unpublished family is outside the new contract; do not retain a hidden
  second listener (FR-1910).

## Requirements

### Functional

- **FR-1901**: THE SYSTEM SHALL accept `proxy_ip_version` as `ipv4` or `ipv6`, defaulting
  to `ipv4`, with environment overriding file configuration and file overriding defaults.
- **FR-1902**: WHEN setup of the preferred loopback listener fails THE SYSTEM SHALL
  release any partially prepared listener and attempt the opposite loopback family once
  before deciding that proxy startup failed.
- **FR-1903**: THE SYSTEM SHALL use the configured `proxy_port` for each family attempt,
  with zero requesting an OS-chosen ephemeral port for that attempt.
- **FR-1904**: WHEN a listener is ready THE SYSTEM SHALL publish its actual literal IP
  address and bound port through `Server::addr()` and all four injected HTTP proxy
  environment variables, with IPv6 bracketed in URLs and the invocation credential retained.
- **FR-1905**: WHEN fallback succeeds THE SYSTEM SHALL emit one `sandme:`-prefixed stderr
  diagnostic identifying the failed preferred address, its cause, and the selected fallback address.
- **FR-1906**: IF both family setups fail THEN THE SYSTEM SHALL exit 125 with one stderr
  startup diagnostic identifying both attempted addresses and causes, without launching the child.
- **FR-1907**: WHEN the sandboxed command exits THE SYSTEM SHALL stop accepting new
  connections on the selected listener.
- **FR-1908**: WHILE the proxy is disabled THE SYSTEM SHALL attempt neither listener family.
- **FR-1909**: IF `proxy_ip_version` is present with a value other than `ipv4` or `ipv6`
  THEN THE SYSTEM SHALL exit 125 with a config diagnostic naming the setting and accepted values,
  without launching the child.
- **FR-1910**: WHEN preferred listener setup succeeds THE SYSTEM SHALL serve only that
  loopback family and SHALL NOT attempt setup of the opposite family.

### Non-functional

- **NFR-1901**: THE SYSTEM SHALL retain authentication, egress restrictions, HTTP forwarding
  and SSH CONNECT behavior for both possible selected families, with 0 changes to their
  authorization decisions and 0 new Cargo dependencies.

## Interface contract

**Configuration**

| Key | Env var | Type | Default | Description |
| --- | --- | --- | --- | --- |
| `proxy_ip_version` | `SANDME_PROXY_IP_VERSION` | string: `ipv4` or `ipv6` | `ipv4` | Preferred listener family; the opposite is always fallback on setup failure. |

Values are lowercase and exact; an empty value is invalid. File values of another type are
invalid. No new CLI option is introduced. Existing `proxy_port` and proxy-enabled defaults
remain as implemented; family preference does not select an outbound destination family.

**Listener selection**

| Preference | First attempt | Fallback if first setup fails |
| --- | --- | --- |
| `ipv4` (default) | `127.0.0.1:<configured-port>` | `[::1]:<configured-port>` |
| `ipv6` | `[::1]:<configured-port>` | `127.0.0.1:<configured-port>` |

Availability is determined by completing real listener setup. There is no separate probe,
DNS lookup, route inspection or internet-connectivity check. Stop after the first ready
listener. The existing `serve(&Config) -> Result<Server, SandmeError>` signature remains.

**Exit codes / errors**

| Outcome | Status | Diagnostic |
| --- | --- | --- |
| Preferred setup succeeds | Child status | No listener-selection diagnostic. |
| Fallback succeeds | Child status | `sandme: proxy could not listen on <preferred-address>: <cause>; continuing on <actual-fallback-address>` |
| Both setups fail | `125` | `sandme: proxy could not listen on either loopback address: <preferred-address>: <cause>; <fallback-address>: <cause>` |
| Invalid family setting | `125` | `sandme: config error: invalid proxy_ip_version; expected ipv4 or ipv6` (file decoding may add source context). |

All diagnostics go to stderr and exclude proxy credentials. Failed attempt addresses use
the requested port, including zero; successful fallback addresses use the actual bound port.
Family selection alone does not widen access and introduces no configuration-provenance warning.

## Constraints and dependencies

- Keep configuration and parsing in `config`; listener selection and lifetime in `proxy/server`;
  user-facing error ownership in `error`, consistent with ADR-0001's module boundaries.
- Preserve credential and destination policy before any forwarding; listener family is independent
  of the destination's IP family.
- Preserve the sandbox backends' existing loopback/port grants and TCP limitations; verify the
  selected address works from the actual sandbox on each supported OS. Do not claim unrun Linux checks.
- Replace dual-listener-specific tests and active verification pointers only when this spec
  is accepted; preserve their authentication and egress assertions for each selected family.
- The dual-listener preservation instruction in ADR-0001 D5 governed the previous relocation;
  this is an explicitly approved subsequent behavior change, without changing component boundaries.

## Success criteria

- **SC-1901**: Scenarios 1–4 demonstrate default selection, explicit selection, precedence
  and rejected invalid input (FR-1901, FR-1909).
- **SC-1902**: Scenarios 5–7 demonstrate both fallback directions and total failure
  without a child launch (FR-1902, FR-1903, FR-1905, FR-1906).
- **SC-1903**: Scenarios 8–11 demonstrate actual address publication, listener shutdown,
  proxy-disabled behavior, one-family service and unchanged policy/SSH (FR-1904,
  FR-1907, FR-1908, FR-1910, NFR-1901).

## Verification

The tests below implement the accepted verification plan. Live loopback tests use
controlled listeners and temporary directories; no internet or editor dependency is required.

| Requirement | Verified by |
| --- | --- |
| FR-1901 | Config tests `defaults_proxy_ip_version_to_ipv4`, `loads_proxy_ip_version_from_file`, `environment_overrides_proxy_ip_version`; CLI `uses_the_preferred_loopback_family` for both preferences |
| FR-1902 | CLI `falls_back_to_ipv6_when_ipv4_is_unavailable`, `falls_back_to_ipv4_when_ipv6_is_unavailable`; listener-setup error paths reviewed for release before retry |
| FR-1903 | Both fallback tests assert configured fixed port; CLI `publishes_the_selected_ephemeral_proxy_address` asserts nonzero actual port for both families |
| FR-1904 | `publishes_the_selected_ephemeral_proxy_address` validates all four environment variables and credentials; policy/relay and SSH cases below exercise those URLs |
| FR-1905 | Both fallback tests assert one stderr diagnostic with failed address, cause and actual selected address; preferred-success test asserts no selection warning |
| FR-1906 | Update `reports_a_proxy_that_cannot_bind_with_the_reserved_status` to occupy both families, assert both causes and an absent child marker |
| FR-1907 | Parameterize existing `proxy_lifetime_follows_command` over both preferences |
| FR-1908 | Existing `proxy_disabled_wires_no_proxy_and_denies_egress`, extended to both preferences and absence of fallback diagnostics |
| FR-1909 | Config/CLI invalid-input cases `rejects_invalid_proxy_ip_version`, covering invalid file string/type and invalid/empty environment values; assert no child marker |
| FR-1910 | `uses_the_preferred_loopback_family` confirms the opposite free address is not served by sandme and an occupied opposite address causes no warning |
| NFR-1901 | Parameterize credential refusal, private-address rejection, authenticated HTTP relay and `reaches_an_origin_through_the_ssh_connect_tunnel` over both selected families; compare Cargo manifest and lockfile; run full gate on macOS and Linux or record the unrun platform |

### Validation evidence — 2026-10-29

- macOS full gate passed after the last source edit: `cargo fmt`, `cargo fmt --check`,
  `cargo clippy --all-targets` (0 warnings), `cargo build`, `cargo test` (81 unit +
  66 CLI + 3 tunnel = 150 passed), `cargo doc --no-deps` (0 warnings). The ignored
  `probe` is a documented fixture driven by other tests.
- Standards, repository-wide spec, security and performance reviews completed.
  Review reports are retained locally in the ignored `.agents/reports/` directory
  or maintainer-held private records, not versioned. Security details remain private
  under SECURITY.md. Spec review verified all 11 requirements on macOS.
  Review completion is not security merge approval.
- Release performance review measured 30 runs after 3 warmups per case: default IPv4
  9.875 ms, preferred IPv6 9.927 ms, fallback to IPv6 9.871 ms, fallback to IPv4
  9.901 ms medians. Bare `true` was 1.739 ms. No timing budget or speedup is claimed.
- Linux build/runtime enforcement and startup timing were not executed on this macOS
  host. This is a verification gap, not a Linux passing result.
- Security findings and reproduction details are retained privately in accordance
  with SECURITY.md. Maintainer security disposition is required before merge.
  Passing functional tests and this spec's Implemented status do not establish
  security approval or mean that review findings have been fixed.

## Assumptions

- The requested default and preference apply to the local proxy listener only.
- Either family may fall back, including IPv4 to IPv6, as requested by automatic detection.
- Fallback applies to any listener setup error; no special error allowlist or repeated retries.
- `proxy_ip_version`, lowercase values, symmetric warnings, and strict validation of this
  new setting are the approved interface choices.
- A single listener replaces the compatibility listener for clients that independently
  try IPv6 despite an IPv4 proxy URL; such clients must honor the actual published address.

## Open questions

None. The user approved this spec on 2026-10-29.

## Implementation tasks

- [x] **T-1901** — At acceptance, annotate superseded baseline clauses and update active
      verification pointers without rewriting their original requirements (covers FR-1902,
      FR-1904, FR-1905, FR-1906, FR-1910).
- [x] **T-1902** — Add typed family preference, file/env loading and validation with
      config tests; preserve existing key behavior (covers FR-1901, FR-1909).
- [x] **T-1903** — Add or update selection/fallback integration tests alongside one-listener
      startup and cleanup; retain `serve`, `addr` and `url` signatures (covers FR-1902
      through FR-1908 and FR-1910).
- [x] **T-1904** — Verify authenticated HTTP, restricted destinations and SSH CONNECT for
      both selected families; update user reference and config examples (covers FR-1901,
      FR-1904, FR-1909, NFR-1901).
- [x] **T-1905** — Run the full quality gate, standards review and required spec, security
      and performance reviews before a PR; record platform gaps and measurements rather
      than inferring success (covers FR-1901 through FR-1910, NFR-1901).

## Changelog

| Date | Change |
| --- | --- |
| 2026-10-29 | Initial Draft for IPv4 default, configurable IPv6 preference and automatic fallback to the other family. Implementation awaits spec approval. |
| 2026-10-29 | User approved SPEC-0019; status → Accepted. Interface assumptions accepted; implementation started. |
| 2026-10-29 | Implemented typed family preference, single-listener setup with symmetric fallback, actual address publication and diagnostics; all verification tests and reviews completed on macOS. Linux execution gap and private security-disposition requirement recorded above. Status → Implemented. |
