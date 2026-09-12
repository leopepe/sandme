# SPEC-0003: Proxy egress restrictions

## Metadata

- **Status**: Implemented
- **Created**: 2026-09-06
- **Updated**: 2026-09-12
- **Related ADRs**: none yet
- **Related specs**: SPEC-0001 (extends; narrows one of its non-goals — see Spec deltas)

## Summary

sandme's proxy runs unsandboxed and relays whatever it is asked to relay, to anyone who
connects to it. The sandbox profile denies the sandboxed command a direct connection to the
host's loopback, to the LAN and to link-local addresses; the proxy reaches all of them on the
command's behalf, and any other local process can use it while a run is in flight. This spec
makes the proxy refuse those destinations, requires a per-invocation credential on every
request, and adds one configuration key for the workflows that genuinely need a local
destination.

## Spec deltas

### MODIFIED

- **SPEC-0001/Non-goals** — was: *"Inspecting, filtering or blocking traffic at the proxy —
  this spec only requires that traffic be routed through it. Any policy layer is a later
  spec."* Now: the proxy blocks a bounded, fixed set of **destinations** (FR-201) and refuses
  requests that do not carry this invocation's credential (FR-203). Inspecting traffic, and a
  configurable destination allowlist, remain out of scope. Reason: SPEC-0001 assumed routing
  and restriction were separable. Issue
  [#15](https://github.com/leopepe/sandme/issues/15) shows they are not — with an
  unrestricted relay reachable from inside, `(allow network-outbound (remote ip
  "localhost:N"))` describes routing rather than confinement, and the destinations the
  profile denies are reachable anyway. This is the "later spec" the non-goal anticipated,
  scoped to the smallest policy that makes the profile mean what it reads as.

### ADDED

- **FR-201** … **FR-206**, **NFR-201** … **NFR-202** below.

SPEC-0001's requirements are unchanged. FR-006 (egress is routed through the proxy) still
holds; what a routed request may reach is now bounded.

## Problem

Verified on macOS 26 against `45e97bf`, and reproduced on `65a2c42` while writing this spec.

**1. The proxy is a pivot.** With a host-only service listening on `127.0.0.1:19555`:

```
sandboxed, direct (curl --noproxy '*'):  curl: (7) Failed to connect to 127.0.0.1 port 19555
sandboxed, through the proxy:            HOST-ONLY-INTERNAL-SECRET   http_code=200
```

The same relay reaches RFC1918 (`192.168.88.218:19556` answered `LAN-ONLY-SERVICE` from
inside the sandbox) and link-local: a request for `http://169.254.169.254/latest/meta-data/`
— the cloud metadata endpoint — is attempted and answered `502` only because nothing is
listening on this laptop. On a host where something *is* listening, it is relayed.

So egress is routed, not restricted. Every destination the host can reach, the sandboxed
process can reach, including the ones the profile appears to deny — and the profile reads as
though it denies them.

**2. The proxy relays for anyone.** It binds loopback with no authentication and no check on
who connected. While `sandme 'sleep 8'` runs, from an ordinary unsandboxed shell:

```
curl -x http://127.0.0.1:18988 http://example.com/    -> 200
curl -x http://127.0.0.1:18988 https://example.com/   -> 200
```

The impact on a single-user laptop is small — a process that can do this can open its own
socket — but the proxy is an ambient open relay for the duration of every run, and
`proxy_port` defaulting to `8787` makes it trivially discoverable.

**3. Blocking the first without an escape hatch breaks a real workflow.** `sandme claude`
against a local model server (`http://127.0.0.1:11434`), or any locally hosted API, is
exactly the connection FR-201 refuses. That workflow needs a way to say yes, explicitly.

## Goals

- Make the sandbox profile's network denial mean what it reads as: what the child cannot
  reach directly, it cannot reach through the proxy either.
- Scope the relay to the process sandme launched, for the invocation that launched it.
- Give the locally-hosted-service workflow one documented, explicit opt-out that is off by
  default.

## Non-goals

- A configurable destination allowlist or blocklist — the general policy layer SPEC-0001
  anticipated. FR-201's set is fixed and not user-editable; the only knob is FR-205's
  all-or-nothing opt-out.
- Inspecting, logging or rewriting relayed traffic, or terminating TLS.
- Per-destination or per-process authorisation. Any process holding the credential is the
  child as far as the proxy is concerned.
- Making the proxy optional (`SANDME_PROXY=off`, [#10](https://github.com/leopepe/sandme/issues/10)
  item 1) — complementary, not this spec.
- The IPv6 listener's port mismatch (#10 item 2), already fixed, and any further work on the
  IPv6 listener.
- Defending against an attacker who already runs code as the same user: they can read the
  child's environment, and therefore the credential.
- Blocking non-HTTP protocols. The sandbox already denies them (SPEC-0001).

## User scenarios *(mandatory)*

### Story 1 — The profile's network denial holds (P1)

As someone sandboxing a tool I do not fully trust, I want the destinations the sandbox denies
directly to stay unreachable through sandme's proxy, so that reading the profile tells me what
the tool can reach.

**Acceptance scenarios**

1. **Given** a service listening on the host's `127.0.0.1`, **When** the sandboxed command
   requests it through the proxy, **Then** the proxy answers `403` and the service's content
   does not reach the command.
2. **Given** the default configuration, **When** the sandboxed command requests
   `http://169.254.169.254/latest/meta-data/`, **Then** the proxy answers `403` without
   attempting a connection.
3. **Given** the default configuration, **When** the sandboxed command requests an ordinary
   external URL, **Then** it is relayed as before.

### Story 2 — The relay is the child's, not the machine's (P1)

As someone running sandme on a shared or busy machine, I want the proxy to serve only the
command sandme started, so that it is not an open relay for the duration of every run.

**Acceptance scenarios**

1. **Given** a sandme invocation in flight, **When** an unrelated local process sends a
   request to the proxy port with no credential, **Then** the proxy answers `407` and relays
   nothing.
2. **Given** the same invocation, **When** the sandboxed command makes a request, **Then** it
   is relayed — the command needed no configuration of its own.

### Story 3 — A locally hosted model still works (P1)

As someone running a coding agent against a local LLM server on `127.0.0.1:11434`, I want one
setting that lets sandme's proxy reach it, so that the safe default does not cost me the
workflow.

**Acceptance scenarios**

1. **Given** `allow_private_egress` enabled, **When** the sandboxed command requests
   `http://127.0.0.1:11434/`, **Then** the request is relayed.
2. **Given** `allow_private_egress` enabled, **When** an unrelated local process sends a
   request to the proxy port with no credential, **Then** it is still refused — the opt-out
   widens destinations, not who may ask.

### Edge cases

- **A hostname that resolves into a restricted range** (`localtest.me`, `*.nip.io`, an
  internal DNS name for a LAN host). Checking only literal IPs would make FR-201 cosmetic, so
  the name is resolved and every address it yields is checked (FR-206).
- **A name that resolves to a mix of restricted and public addresses.** Refused. A partial
  allow would hand the client a choice sandme cannot police.
- **IPv4-mapped and IPv4-compatible IPv6** (`::ffff:127.0.0.1`, `::127.0.0.1`) and the
  unspecified addresses (`0.0.0.0`, `::`, which macOS routes to loopback). All restricted;
  they are the obvious way around a naive check.
- **DNS rebinding on the plain-HTTP path.** sandme resolves the name to vet it and the HTTP
  client resolves it again to connect, so a name whose answer changes between the two can
  still be relayed once. Recorded under Assumptions, not fixed here.
- **A request with no absolute-form authority.** Answered `400`; there is nothing to vet.
- **The proxy's own port.** Loopback, so FR-201 refuses it — the relay cannot be made to
  loop through itself unless FR-205 is enabled.
- **`allow_private_egress` enabled.** FR-201 does not apply; the pivot described in Problem
  §1 is available, by the user's explicit choice, and FR-203 still bounds who may use it.

## Requirements *(mandatory)*

### Functional

- **FR-201**: WHILE `allow_private_egress` is disabled, IF a proxied request names a
  destination that resolves to a loopback, private (RFC1918), link-local, unique-local or
  unspecified address THEN THE SYSTEM SHALL answer `403 Forbidden` and SHALL NOT open a
  connection to that destination.
- **FR-202**: WHEN the proxy refuses a request under FR-201 THE SYSTEM SHALL write one
  diagnostic line to stderr naming the destination and the setting that permits it.
- **FR-203**: THE SYSTEM SHALL generate a credential for each invocation, publish it to the
  sandboxed command in the proxy URL it already sets (FR-006), and answer `407 Proxy
  Authentication Required` to any request that does not present that credential.
- **FR-204**: WHEN the proxy relays a request THE SYSTEM SHALL remove the credential from it,
  so the origin never receives it.
- **FR-205**: WHERE `allow_private_egress` is enabled THE SYSTEM SHALL relay to the
  destinations FR-201 refuses, and SHALL still enforce FR-203.
- **FR-206**: THE SYSTEM SHALL apply FR-201 to `CONNECT` and to absolute-form requests alike,
  and to every address the destination's hostname resolves to.

### Non-functional

- **NFR-201**: The credential SHALL be at least 128 bits drawn from the operating system's
  entropy source, SHALL differ between invocations, and SHALL NOT be written to disk or to
  any log.
- **NFR-202**: The check SHALL cost at most one name resolution per proxied request and SHALL
  add no connection attempt; a refused request performs none.

## Interface contract

**Configuration** *(file keys and matching environment variables)*

| Key | Env var | Type | Default | Description |
| --- | --- | --- | --- | --- |
| `allow_private_egress` | `SANDME_ALLOW_PRIVATE_EGRESS` | boolean (env: `1` or `true`) | `false` | Relay to loopback, RFC1918, link-local, unique-local and unspecified destinations. Needed for a locally hosted service (a local model server, a dev API). Off by default: it re-opens the pivot in Problem §1. |

No new flag and no new exit code. The proxy's answers to the client are:

| Status | Condition | Message to the client |
| --- | --- | --- |
| `403` | FR-201 — the destination is in a restricted range | `sandme: proxy refused <destination>: loopback, private and link-local destinations are not relayed; set allow_private_egress to permit them` |
| `407` | FR-203 — the request carried no valid credential | `sandme: proxy refused a request that did not come from this invocation's sandboxed command` |
| `400` | The request has no destination to vet | `sandme: proxy received a request with no destination` |
| `502` | The destination was permitted but could not be reached | *(unchanged from SPEC-0001)* |

## Key entities *(optional)*

- **Restricted range**: the fixed set of address ranges FR-201 refuses — `127.0.0.0/8`,
  `10.0.0.0/8`, `172.16.0.0/12`, `192.168.0.0/16`, `169.254.0.0/16`, `0.0.0.0`, `::1`,
  `fe80::/10`, `fc00::/7`, `::`, and any IPv4 address embedded in an IPv6 one.
- **Invocation credential**: the per-run secret the proxy requires and the child is given
  through `HTTP_PROXY`/`HTTPS_PROXY`. It lives in memory for the run and nowhere else.

## Constraints and dependencies

- SPEC-0001 FR-006: the child's proxy configuration is `HTTP_PROXY`/`HTTPS_PROXY`. The
  credential travels in those URLs (`http://sandme:<secret>@127.0.0.1:<port>`) because HTTP
  clients already turn proxy-URL userinfo into a `Proxy-Authorization: Basic` header — no
  client learns anything new.
- `std::net::IpAddr` classifies the IPv4 ranges (`is_loopback`, `is_private`,
  `is_link_local`, `is_unspecified`); the IPv6 predicates for `fe80::/10` and `fc00::/7` are
  still unstable in `std`, so they are matched on the address's first segment. No new
  dependency (`AGENTS.md`, Dependencies).
- The proxy runs unsandboxed inside sandme's own process. Nothing in the sandbox profile can
  enforce FR-201; it is enforced in the proxy's own request path or not at all.

## Success criteria *(mandatory)*

- **SC-201**: With the defaults, the probes in Problem §1 return `403` and no host-only
  content, while an ordinary external request still returns `200`.
- **SC-202**: With the defaults, the open-relay probe in Problem §2 returns `407` for both
  plain HTTP and `CONNECT`.
- **SC-203**: With `SANDME_ALLOW_PRIVATE_EGRESS=1`, a sandboxed request to a service on the
  host's loopback succeeds.
- **SC-204**: A sandboxed coding agent or IDE reaches the internet through the proxy exactly
  as before — no configuration, no visible change.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-201 | `refuses_to_relay_to_a_service_on_host_loopback`, `refuses_to_relay_to_the_cloud_metadata_address`, `refuses_to_relay_to_host_loopback_over_the_ipv6_listener` (`tests/cli.rs`, the last proving the check fires on the `[::1]` listener too); `restricts_every_range_the_sandbox_denies_directly`, `relays_to_ordinary_public_addresses` (`src/egress.rs`) |
| FR-202 | `refuses_to_relay_to_a_service_on_host_loopback` (`tests/cli.rs`) asserts the stderr line |
| FR-203 | `refuses_a_local_client_without_the_invocation_credential` and `refuses_a_local_client_on_the_ipv6_listener_without_the_invocation_credential` (`tests/cli.rs`) for the refusal on the IPv4 and the `[::1]` listener; `routes_http_egress_through_the_proxy` and `no_manual_proxy_configuration_needed` (`tests/cli.rs`) for the child being accepted without configuring anything; `publishes_a_secret_the_child_can_present`, `expects_what_a_client_reading_the_proxy_url_sends`, `encodes_base64_at_every_padding_length` (`src/credential.rs`) |
| FR-204 | `relays_to_host_loopback_when_private_egress_is_allowed` (`tests/cli.rs`) — the origin echoes the request it received and it carries no `proxy-authorization` |
| FR-205 | `relays_to_host_loopback_when_private_egress_is_allowed` (`tests/cli.rs`); `opens_private_egress_only_when_the_environment_asks_for_it`, `keeps_defaults_for_keys_a_config_file_omits` (`src/config.rs`) |
| FR-206 | `restricts_a_hostname_that_resolves_into_a_restricted_range` (`src/egress.rs`); `reads_the_destination_a_request_names` (`src/proxy.rs`) covers both request forms and the bracketed IPv6 literal; `refuses_a_local_client_without_the_invocation_credential` and `refuses_a_local_client_on_the_ipv6_listener_without_the_invocation_credential` exercise `CONNECT` on both listeners |
| NFR-201 | `publishes_a_secret_the_child_can_present` (`src/credential.rs`) — 128 bits, hex, different between two invocations |
| NFR-202 | `refuses_to_relay_to_the_cloud_metadata_address` (`tests/cli.rs`) returns in well under the timeout, which it could only do without a connection attempt |

## Assumptions

- **Basic proxy authentication over loopback is enough.** The credential is compared as a
  string, not in constant time, and travels in clear text over loopback. Both are acceptable
  because the whole exchange stays on the local machine and a same-uid attacker already holds
  strictly stronger capabilities (Non-goals).
- **The child may read its own credential.** It is in its environment; that is how it
  authenticates. FR-203 scopes the relay to *an* invocation, not to a process identity.
- **DNS rebinding between the vetting resolution and the client's own resolution is
  accepted** on the plain-HTTP path. Closing it means owning the connection the HTTP client
  makes, which is a larger change than this spec justifies; the `CONNECT` path already
  connects to the vetted addresses only.
- **The restricted set does not need CGNAT (`100.64.0.0/10`), multicast or reserved space.**
  Nothing in #15 reaches them and the sandbox does not treat them specially.
- **Users who need a local destination will read one line in the README.** The alternative —
  a smart default that detects a local model server — is a guess about intent.

## Open questions

None.

## Implementation tasks

- [x] **T-201** — `allow_private_egress` in `Config`, file key and environment override
      (covers FR-205)
- [x] **T-202** — Refuse the restricted ranges in the proxy's `CONNECT` and forward paths,
      with the stderr diagnostic and the `403` body (covers FR-201, FR-202, FR-206)
- [x] **T-203** — Per-invocation credential: generated at `serve`, published in the child's
      proxy URL, required on every request, stripped before relaying (covers FR-203, FR-204,
      NFR-201)
- [x] **T-204** — Tests: unit tests for range classification, credential and header handling;
      integration tests for each refusal and for the opt-out (covers FR-201 … NFR-202)
- [x] **T-205** — README: the new key, what it re-opens, and the local-model recipe

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-06 | Initial draft, covering issue #15. Narrows SPEC-0001's filtering non-goal to the smallest policy that makes the sandbox profile's network rule hold. |
| 2026-09-10 | Added integration tests that exercise FR-201, FR-203 and FR-206 on the `[::1]` loopback listener, not only the IPv4 one. No requirement or behaviour change: the listener already enforced the policy (measured against issue #15's probes); the tests close a verification gap on the address family #10 item 2 reports as historically divergent. |
| 2026-09-12 | Status `Review` → `Accepted` → `Implemented`. The egress restrictions shipped with PR [#28](https://github.com/leopepe/sandme/pull/28); all tasks are ticked and every Verification test exists. Records the approval gate that was skipped when the spec landed at `Review` alongside its implementation ([#34](https://github.com/leopepe/sandme/issues/34) item 1). |
