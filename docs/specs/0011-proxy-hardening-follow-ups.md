# SPEC-0011: Proxy hardening follow-ups

## Metadata

- **Status**: Implemented
- **Created**: 2026-09-12
- **Updated**: 2026-09-12
- **Related ADRs**: none yet
- **Related specs**: SPEC-0003 (extends; closes gaps in FR-201 and NFR-202 — see Spec deltas)

## Summary

A security audit of the egress proxy (issue #32) found five smaller gaps in `src/proxy.rs` and
`src/egress.rs`, none blocking. This spec closes the four that are concrete — the plain-HTTP path
resolving DNS a second time, the IPv6 listener failing silently, carrier-grade NAT and
benchmarking ranges being relayed, and hop-by-hop headers plus a client-chosen `Host` reaching the
origin — and records the fifth as a deliberate no-change decision. The behaviour is expressed as
deltas against SPEC-0003, which these findings are follow-ups to.

## Spec deltas

### ADDED

- **FR-1101** … **FR-1105**, **NFR-1101** below.

### MODIFIED

- **SPEC-0003/NFR-202** — was: *"The check SHALL cost at most one name resolution per proxied
  request …"*. The requirement stood, but the plain-HTTP forward path did not meet it: `route`
  discarded the vetted addresses on the non-`CONNECT` branch and handed the request to a client
  that resolved the destination again. Now enforced on the plain-HTTP path too (FR-1101): the
  forward path connects to the addresses `egress::resolve` already vetted, exactly as `CONNECT`
  does. Reason: the second lookup is a DNS-rebind window between vetting and connecting; only
  `CONNECT` was closed.
- **SPEC-0003/FR-201** — the restricted range set is extended to include carrier-grade NAT
  (`100.64.0.0/10`, RFC 6598) and benchmarking (`198.18.0.0/15`, RFC 2544), gated by
  `allow_private_egress` like the other ranges (FR-1104). Reason: Tailscale assigns tailnet
  addresses from `100.64.0.0/10`, so tailnet peers the sandbox profile denies directly were relayed
  by the proxy — the exact pivot issue #15 was filed about, in a range SPEC-0003 did not cover. This
  overturns the SPEC-0003 assumption *"The restricted set does not need CGNAT … or reserved space."*
  for these two ranges.

## Problem

The four concrete findings, as measured by the audit against PR #28:

1. **The plain-HTTP path resolves twice.** `route` vets a destination by resolving it, then
   discards the resolved addresses and hands the whole request to a hyper client that resolves the
   name again to connect. Between the two lookups the name's answer can change (DNS rebinding), so a
   name vetted as public can be connected to as private. A live leak was not reproduced — macOS's
   resolver cache pins the answer across the sub-millisecond gap — but the window is real. `CONNECT`
   is unaffected: it connects to the vetted `SocketAddr` list, which performs no resolution.
2. **The IPv6 listener fails silently.** The `[::1]` bind error is swallowed with `.ok()`. A process
   already squatting `[::1]:<port>` answers a child's request verbatim, and sandme starts with no
   warning, while the IPv4 bind fails loudly. Bounded — the published proxy URL names the literal
   `127.0.0.1`, so a well-behaved client does not reach the squatter — but a failure a user cannot
   see is a failure they cannot act on.
3. **CGNAT and benchmarking ranges are relayed.** On defaults, `100.64.0.1` and `198.18.0.1` are
   connect-attempted rather than refused. CGNAT is the one that matters: Tailscale uses
   `100.64.0.0/10`, so the proxy relays to tailnet peers the profile denies directly.
4. **Hop-by-hop headers and a client-chosen `Host` reach the origin.** Only `Proxy-Authorization`
   is stripped; `Proxy-Connection: Keep-Alive` was observed passing through, and a client-supplied
   `Host` reached the origin while the vetted authority was different. No IP-level bypass — the
   connection still goes where vetting allowed — but it is vhost selection by the client and a small
   request-smuggling surface.

The fifth finding (credential comparison is not constant-time; the `/dev/urandom` fallback uses
`RandomState`) is recorded under Non-goals: the audit judged it below the noise floor and reachable
only by an attacker who already holds the credential.

## Goals

- The plain-HTTP forward path connects to the addresses already vetted, resolving at most once per
  request — the same guarantee `CONNECT` gives.
- A failed IPv6 loopback bind is visible to the user, not swallowed.
- Carrier-grade NAT and benchmarking destinations are refused by default and permitted only under
  `allow_private_egress`, like every other restricted range.
- Hop-by-hop headers do not reach the origin, and the origin's `Host` is the authority sandme
  vetted, not one the client chose.

## Non-goals

- **Constant-time credential comparison, and 128 fresh bits in the `/dev/urandom` fallback**
  (finding 5). Left unchanged deliberately: the timing signal is far below loopback/hyper/tokio
  noise, anyone positioned to measure it can already read the credential from the child's
  environment (issue #31), and the `RandomState` fallback is reachable only if `/dev/urandom` will
  not open. Changing either adds code without closing a reachable gap — see
  `docs/guidelines/code/simplicity.md` §4.
- **Multicast, and reserved space other than benchmarking.** Nothing in issue #15 or issue #32
  reaches them; only CGNAT and benchmarking were measured as relayed and only CGNAT carries a named
  pivot (Tailscale).
- **Failing the invocation when the IPv6 bind fails.** The IPv6 listener is a best-effort second
  listener for clients that try `[::1]` on their own; the published URL is IPv4. A warning is the
  proportionate response — see Assumptions.
- **A configurable destination allowlist**, still out of scope per SPEC-0003.

## User scenarios *(mandatory)*

### Story 1 — What the profile denies stays denied, including the tailnet (P1)

As someone sandboxing a tool with a tailnet on the same machine, I want the destinations the
sandbox denies directly to stay unreachable through the proxy, including carrier-grade NAT, so that
reading the profile tells me what the tool can reach.

**Acceptance scenarios**

1. **Given** the default configuration, **When** the sandboxed command requests
   `http://100.64.0.1/`, **Then** the proxy answers `403` without attempting a connection.
2. **Given** the default configuration, **When** the sandboxed command requests
   `http://198.18.0.1/`, **Then** the proxy answers `403` without attempting a connection.
3. **Given** `allow_private_egress` enabled, **When** a CGNAT destination is resolved, **Then** it
   is permitted — the opt-out widens destinations uniformly.

### Story 2 — A failed IPv6 listener is visible (P1)

As someone starting sandme on a machine where `[::1]:<port>` is already taken, I want to be told the
IPv6 loopback listener could not bind, so that a squatter answering `[::1]` is not a silent
condition.

**Acceptance scenarios**

1. **Given** a process squatting `[::1]:<port>` with IPv4 loopback on that port free, **When**
   sandme starts its proxy there, **Then** it writes a `sandme:`-prefixed diagnostic naming the
   IPv6 loopback address and continues serving on IPv4.

### Story 3 — The origin sees only what the proxy vetted (P1)

As someone sandboxing a tool, I want the origin to receive the `Host` sandme vetted and none of the
hop-by-hop headers, so that a client cannot select a vhost or smuggle proxy-scoped headers past the
proxy.

**Acceptance scenarios**

1. **Given** a relayed plain-HTTP request carrying a client-chosen `Host` and a `Proxy-Connection`
   header, **When** it reaches the origin, **Then** the origin sees the vetted authority as `Host`
   and does not see `Proxy-Connection`.

### Edge cases

- **A name that resolves into CGNAT or benchmarking space.** Refused whole by default, like any
  restricted range (SPEC-0003 FR-206 is unchanged and applies to the added ranges).
- **A destination one address outside the added ranges** (`100.63.255.255`, `100.128.0.1`,
  `198.17.255.255`, `198.20.0.1`). Not restricted by the addition.
- **The IPv6 bind failing while IPv4 succeeds.** Warn and continue; the child still has a working
  IPv4 proxy.
- **A `Connection` header naming further fields.** Those fields are hop-by-hop too (RFC 9110
  §7.6.1) and are removed with it.

## Requirements *(mandatory)*

### Functional

- **FR-1101**: WHEN the proxy forwards a plain-HTTP (absolute-form) request THE SYSTEM SHALL open
  the connection to the addresses vetted for its destination and SHALL NOT resolve the destination
  a second time.
- **FR-1102**: WHEN the proxy forwards a plain-HTTP request THE SYSTEM SHALL remove the hop-by-hop
  header fields — `Connection` and the fields it names, `Keep-Alive`, `Proxy-Connection`,
  `Proxy-Authenticate`, `Proxy-Authorization`, `TE`, `Trailer`, `Transfer-Encoding` and `Upgrade`
  (RFC 9110 §7.6.1) — before relaying it.
- **FR-1103**: WHEN the proxy forwards a plain-HTTP request THE SYSTEM SHALL set the `Host` header
  to the authority it vetted.
- **FR-1104**: WHILE `allow_private_egress` is disabled, IF a proxied request resolves to a
  carrier-grade NAT (`100.64.0.0/10`) or benchmarking (`198.18.0.0/15`) address THEN THE SYSTEM
  SHALL answer `403 Forbidden` and SHALL NOT open a connection to it.
- **FR-1105**: IF the IPv6 loopback listener cannot bind THEN THE SYSTEM SHALL write one
  `sandme:`-prefixed diagnostic line to stderr naming the IPv6 loopback address, and SHALL continue
  serving on IPv4 loopback.

### Non-functional

- **NFR-1101**: The plain-HTTP forward path SHALL perform no name resolution of its own; the one
  resolution SPEC-0003 NFR-202 permits is the vetting resolution in `egress::resolve`.

## Interface contract

No new flag, configuration key or exit code. `allow_private_egress` (SPEC-0003) now also gates the
CGNAT and benchmarking ranges. The proxy's status codes to the client are unchanged: `403` for a
restricted destination (now including CGNAT and benchmarking), `407`, `400`, `502` as in SPEC-0003.

A new diagnostic line is written to stderr when the IPv6 loopback listener cannot bind:

| Stream | Condition | Message |
| --- | --- | --- |
| stderr | FR-1105 — the `[::1]` listener could not bind | `sandme: proxy could not also listen on IPv6 loopback [::1]:<port>: <cause>; continuing on IPv4 loopback only` |

## Key entities *(optional)*

- **Restricted range** (SPEC-0003): the fixed set FR-201 refuses, now extended with `100.64.0.0/10`
  (carrier-grade NAT) and `198.18.0.0/15` (benchmarking).
- **Hop-by-hop header**: a header meaningful only on the single connection it arrived on, listed in
  RFC 9110 §7.6.1, which a proxy must not forward.

## Constraints and dependencies

- `hyper` (already in `Cargo.toml`) provides the low-level `client::conn::http1` handshake the
  forward path uses to speak HTTP/1 over a `TcpStream` it opened to the vetted addresses. No new
  dependency. The `hyper-util` legacy client is no longer used and its `client-legacy`/`http1`
  features are dropped.
- `std::net::Ipv4Addr` has no CGNAT or benchmarking predicate; the two ranges are matched by a
  prefix comparison against named network constants, as SPEC-0003 already does for the IPv6 ranges
  `std` cannot classify.
- The IPv6 listener runs on the tokio runtime inside sandme's own process; the warning is written
  before the child is spawned, on the startup path.

## Success criteria *(mandatory)*

- **SC-1101**: On defaults, `http://100.64.0.1/` and `http://198.18.0.1/` through the proxy return
  `403` and no content, promptly (no connection attempt).
- **SC-1102**: With `[::1]:<port>` squatted and IPv4 free, sandme prints the IPv6 warning and the
  child still runs.
- **SC-1103**: A relayed plain-HTTP request reaches the origin with the vetted `Host` and without
  `Proxy-Connection`, and the ordinary relay path (SPEC-0003 SC-204) is unchanged.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-1101 | `routes_http_egress_through_the_proxy`, `relays_to_host_loopback_when_private_egress_is_allowed` (`tests/cli.rs`) — the plain-HTTP relay still reaches the vetted origin through the rewritten forward path |
| FR-1102 | `strips_hop_by_hop_headers_and_pins_host_to_the_vetted_authority` (`tests/cli.rs`) — the origin's echo does not carry `Proxy-Connection` |
| FR-1103 | `strips_hop_by_hop_headers_and_pins_host_to_the_vetted_authority` (`tests/cli.rs`) — the origin's echo carries the vetted authority as `Host`, not the client's value |
| FR-1104 | `refuses_to_relay_to_a_cgnat_destination_by_default`, `refuses_to_relay_to_a_benchmarking_destination_by_default` (`tests/cli.rs`); `restricts_every_range_the_sandbox_denies_directly`, `relays_to_ordinary_public_addresses`, `gates_cgnat_and_benchmarking_on_private_egress` (`src/egress.rs`) |
| FR-1105 | `warns_when_the_ipv6_loopback_listener_cannot_bind` (`tests/cli.rs`) |
| NFR-1101 | FR-1101's tests, plus construction: `forward` receives the vetted `&[SocketAddr]` and calls no resolver |

## Assumptions

- **A warning, not a hard failure, is the right response to a failed IPv6 bind.** The published URL
  is the literal IPv4 loopback, so a well-behaved client never reaches a `[::1]` squatter; the IPv6
  listener exists only for clients that independently prefer `[::1]`. Denying the invocation over a
  best-effort second listener would cost the common case to defend a corner the URL does not point
  at. The warning makes the condition actionable without that cost.
- **Pinning `Host` to the vetted authority is acceptable to real clients.** A correct client already
  sends `Host` equal to the request authority; pinning changes only the mismatched case, which is
  the one this closes.

## Open questions

None.

## Implementation tasks

- [x] **T-1101** — Connect the plain-HTTP forward path to the vetted addresses via the low-level
      `hyper` http1 client; drop the `hyper-util` legacy client and its features (covers FR-1101,
      NFR-1101)
- [x] **T-1102** — Strip the RFC 9110 hop-by-hop header set and pin `Host` to the vetted authority
      on the forward path (covers FR-1102, FR-1103)
- [x] **T-1103** — Add CGNAT and benchmarking to the restricted ranges in `src/egress.rs`, gated by
      `allow_private_egress` (covers FR-1104)
- [x] **T-1104** — Warn on a failed IPv6 loopback bind and continue on IPv4 (covers FR-1105)
- [x] **T-1105** — Tests: integration tests for the CGNAT/benchmarking `403`, the IPv6 warning and
      the header handling; unit tests for the added range classification and its gating (covers
      FR-1101 … FR-1105, NFR-1101)

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-12 | Initial draft, covering issue #32 findings 1–4 as deltas to SPEC-0003; finding 5 recorded as a deliberate non-goal. |
| 2026-09-12 | Status `Draft` → `Accepted` → `Implemented`. Shipped with PR [#49](https://github.com/leopepe/sandme/pull/49); every Verification test and its supporting production code is present on `main`. Ticked T-1101…T-1105, whose boxes were left unchecked when the work landed ([#34](https://github.com/leopepe/sandme/issues/34)). |
