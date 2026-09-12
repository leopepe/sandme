# SPEC-0012: git-over-SSH via CONNECT tunnel

## Metadata

- **Status**: Draft <!-- Draft | Review | Accepted | Implemented | Superseded -->
- **Created**: 2026-09-12
- **Updated**: 2026-09-12
- **Related ADRs**: none yet
- **Related specs**: SPEC-0001 (extends; reuses FR-006 egress routing and NFR-002 transparent egress), SPEC-0003 (depends on; reuses the per-invocation proxy credential, FR-203)

## Summary

An SSH remote (`git@github.com:…`) is the default for most repositories, but the
Seatbelt profile permits exactly one network destination — the proxy — so `ssh`
to port 22 is denied and `HTTP_PROXY` means nothing to `ssh`. `git fetch`,
`pull` and `push` therefore fail inside the sandbox with an opaque error. This
spec makes git-over-SSH reach its remote by tunnelling `ssh` through sandme's
proxy over HTTP `CONNECT`, wired the same transparent way `HTTP_PROXY` already
is: sandme sets `GIT_SSH_COMMAND` to an `ssh` whose `ProxyCommand` re-executes
sandme in a tunnel mode, carrying this invocation's proxy credential. It does
not widen the sandbox — `ssh` still reaches only the proxy, which relays.

## Spec deltas

### ADDED

- **FR-1201** … **FR-1206**, **NFR-1201** … **NFR-1203** below.

SPEC-0001 and SPEC-0003 are unchanged. FR-006 (egress is routed through the
proxy) and FR-203 (the per-invocation credential in `HTTP_PROXY`) both still
hold; this spec adds a second consumer of them — `ssh` — alongside HTTP clients.

## Problem

Reproduced on `main` (`c33844d`), macOS 26 / Darwin 25.6.0, Apple silicon:

```
$ sandme 'GIT_SSH_COMMAND="ssh -o BatchMode=yes -o ConnectTimeout=8" \
          git ls-remote git@github.com:leopepe/sandme.git'
ssh: connect to host github.com port 22: Operation not permitted
fatal: Could not read from remote repository.
```

The sandbox permits `(allow network-outbound (remote ip "localhost:N"))` and
nothing else, so a direct connection to `github.com:22` is denied. `ssh` reads
neither `HTTP_PROXY` nor any environment variable for a proxy, so the routing
that carries every HTTP client through the proxy does not reach it.

An SSH remote is what `gh repo clone` and most setup guides produce, so for a
large fraction of real repositories `git fetch`/`pull`/`push` cannot work inside
the sandbox — including from an editor's UI, where the failure surfaces as an
opaque error. The README's coding-agent recipe implies an agent can work on a
project; an agent that cannot fetch is substantially less useful (issue #33,
split from #29).

The HTTPS workaround (switching the remote to `https://…`) is not free either:
it pushes work onto the user and then needs credentials, and the sandbox denies
`~/Library/Keychains` (SPEC-0002/FR-102). So the transparent fix is worth the
work.

## Goals

- `git fetch`/`pull`/`push`/`ls-remote` against an SSH remote works inside the
  sandbox with no configuration by the user.
- The mechanism reuses the proxy and the per-invocation credential already in
  `HTTP_PROXY`, so nothing new is written to disk or exposed on a command line.
- The sandbox is not widened: `ssh` reaches only the proxy, exactly as every
  other egress does.

## Non-goals

- Tunnelling raw `ssh` invoked outside git. `ssh` has no environment variable
  for a `ProxyCommand`, and sandme does not write an `ssh_config`; only git's
  `GIT_SSH_COMMAND` path is wired. A user running `ssh` directly can add
  `-o ProxyCommand` themselves (the same value sandme sets).
- Permitting direct port-22 egress via an SSH allowlist (issue #33 option 2).
  That widens the Seatbelt profile and is a separate user decision.
- Supplying SSH credentials or host keys. Reaching the remote still needs the
  user's key and `known_hosts` to be within a shared path (SPEC-0007); this
  spec removes the network denial, not the filesystem confinement.
- Constant-time credential comparison, TLS to the proxy, or any change to the
  credential itself — inherited unchanged from SPEC-0003.

## User scenarios *(mandatory)*

### Story 1 — An SSH remote reaches GitHub (P1)

As someone whose repository has a `git@github.com:…` remote, I want `git` to
reach it inside the sandbox without changing my remote or my configuration, so
that an agent or editor under sandme can fetch and push.

**Acceptance scenarios**

1. **Given** a sandboxed command and no `GIT_SSH_COMMAND` of the user's own,
   **When** the command inspects its environment, **Then** `GIT_SSH_COMMAND` is
   set to an `ssh` whose `ProxyCommand` re-executes sandme in tunnel mode.
2. **Given** an origin reachable only through the proxy, **When** the sandboxed
   command runs sandme's `ProxyCommand` mode against it, **Then** bytes are
   relayed both ways across an HTTP `CONNECT` tunnel and the origin's answer
   returns.

### Story 2 — The user's own SSH configuration wins (P2)

As someone who has already set `GIT_SSH_COMMAND`, I want sandme to leave it
alone, so that my configuration is not silently overwritten.

**Acceptance scenarios**

1. **Given** `GIT_SSH_COMMAND` set before invoking sandme, **When** the
   sandboxed command inspects its environment, **Then** the value is unchanged.

### Edge cases

- **The proxy declines the `CONNECT`.** A `403` (the destination is on a range
  the proxy refuses, SPEC-0003 FR-201) or `407` (missing credential) is
  reported on stderr with the destination and status, and the `ProxyCommand`
  exits non-zero so `ssh` fails cleanly rather than hanging (FR-1205).
- **sandme cannot find its own executable.** The `ProxyCommand` re-executes
  sandme by absolute path; with no path to name, `GIT_SSH_COMMAND` is left
  unset (FR-1206) and the pre-existing opaque failure remains — no worse than
  today.
- **The user's key or `known_hosts` is outside every shared path.** Out of
  scope: the tunnel establishes the transport and `ssh` then fails at
  authentication or host-key verification, which is progress past the network
  denial and the user's to resolve by sharing `~/.ssh` (SPEC-0007).

## Requirements *(mandatory)*

### Functional

- **FR-1201**: WHEN sandme launches a sandboxed command AND the user has not set
  `GIT_SSH_COMMAND` THE SYSTEM SHALL set `GIT_SSH_COMMAND` in the child so that
  git's `ssh` tunnels through the proxy over HTTP `CONNECT`.
- **FR-1202**: IF the user has already set `GIT_SSH_COMMAND` THEN THE SYSTEM
  SHALL leave it unchanged.
- **FR-1203**: WHEN sandme is run as ssh's `ProxyCommand` THE SYSTEM SHALL open
  an HTTP `CONNECT` tunnel to the requested host and port through the proxy named
  in `HTTP_PROXY`, presenting that invocation's credential (SPEC-0003 FR-203).
- **FR-1204**: WHILE the tunnel is open THE SYSTEM SHALL relay bytes in both
  directions between its standard input/output and the tunnel until either end
  closes.
- **FR-1205**: IF the proxy answers the `CONNECT` with a non-`2xx` status THEN
  THE SYSTEM SHALL write one `sandme:`-prefixed diagnostic to stderr naming the
  destination and the status, and SHALL exit non-zero.
- **FR-1206**: IF sandme cannot determine its own executable path THEN THE
  SYSTEM SHALL leave `GIT_SSH_COMMAND` unset.

### Non-functional

- **NFR-1201**: The tunnel SHALL require no configuration by the user — the
  transparent-egress property of SPEC-0001 NFR-002 extended to git-over-SSH.
- **NFR-1202**: The tunnel SHALL NOT widen the Seatbelt profile. `ssh`'s egress
  SHALL leave only through the proxy; the profile SHALL permit no network
  destination other than the proxy (no port-22 rule, `src/profile.rs`
  unchanged).
- **NFR-1203**: The credential SHALL NOT appear in the `GIT_SSH_COMMAND` value
  or on any command line; the `ProxyCommand` reads it from `HTTP_PROXY` at run
  time, preserving SPEC-0003 NFR-201 (the secret is not written where another
  process can read it more easily than it already can read the environment).

## Interface contract

**Environment** *(injected into the child)*

| Variable | Set to | When |
| --- | --- | --- |
| `GIT_SSH_COMMAND` | `ssh -o ProxyCommand='<sandme> --sandme-ssh-connect %h %p'` | The user has not set it, and sandme knows its own path |

`--sandme-ssh-connect <host> <port>` is not a documented flag: it is sandme's
private convention for its `ProxyCommand` mode, intercepted before the CLI is
parsed. `%h`/`%p` are ssh's own destination placeholders.

**Exit codes / errors** *(the `ProxyCommand` mode)*

| Code | Condition | Message to user |
| --- | --- | --- |
| `0` | The tunnel opened and relayed until close | — |
| `125` | `HTTP_PROXY` unusable | `sandme: SSH tunnel: no usable HTTP_PROXY in the environment` |
| `125` | The proxy could not be reached | `sandme: SSH tunnel: could not reach the proxy at <addr>: <cause>` |
| `125` | The proxy declined the `CONNECT` | `sandme: SSH tunnel: proxy declined CONNECT to <host>:<port> (HTTP <status>)` |

## Key entities *(optional)*

- **SSH tunnel** (`src/tunnel.rs`): the two halves of one contract — the builder
  of the `GIT_SSH_COMMAND` value, and the `ProxyCommand` runner that opens the
  `CONNECT` tunnel and relays stdio. Kept in one module so the sentinel, the
  argument order and the proxy protocol cannot drift apart.

## Constraints and dependencies

- SPEC-0003 FR-203: the proxy requires a `Proxy-Authorization: Basic` credential
  on every request, published to the child in `HTTP_PROXY`. The `ProxyCommand`
  reads it back from there and re-encodes it with the same base64 the credential
  module uses.
- SPEC-0001 FR-006 / the profile's single `network-outbound` rule: the tunnel
  connects only to the proxy, so no profile rule is added. The profile's
  unscoped `(allow process-exec)` already lets the sandboxed `ssh` re-execute
  the sandme binary as its `ProxyCommand`, wherever that binary lives — measured
  on macOS 26: a binary outside every `file-read*` grant still execs.
- The proxy relays `CONNECT` to public hosts already (SPEC-0003); `github.com:22`
  is public, so `allow_private_egress` is not required.

## Success criteria *(mandatory)*

- **SC-1201**: `sandme 'git ls-remote git@github.com:leopepe/sandme.git'`, with
  the user's `~/.ssh` and `~/.gitconfig` shared, lists the remote's refs and
  exits `0` — where before the fix it failed with `connect to host github.com
  port 22: Operation not permitted`.
- **SC-1202**: With no user configuration, the child's `GIT_SSH_COMMAND` names
  the tunnel `ProxyCommand`.
- **SC-1203**: `src/profile.rs` is unchanged; the profile still permits no
  network destination but the proxy.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-1201 | `wires_the_git_ssh_command_into_the_child` (`tests/cli.rs`); `builds_a_git_ssh_command_that_names_the_proxy_command_sentinel` (`src/tunnel.rs`) |
| FR-1202 | `keeps_a_user_supplied_git_ssh_command` (`tests/cli.rs`) |
| FR-1203 | `reaches_an_origin_through_the_ssh_connect_tunnel` (`tests/cli.rs`) — the CONNECT is authorised by the credential from `HTTP_PROXY`; manual measurement SC-1201 against real GitHub |
| FR-1204 | `reaches_an_origin_through_the_ssh_connect_tunnel` (`tests/cli.rs`) — the origin's reply returns across the tunnel |
| FR-1205 | Manual: `route`/`unauthorized`/`refuse` (`src/proxy.rs`) answer the non-2xx; the tunnel maps it to `SandmeError::TunnelRefused` (`src/tunnel.rs`) |
| FR-1206 | `wire_git_ssh` guards on `current_exe` (`src/sandbox.rs`); covered by inspection — a machine where `current_exe` fails is not reproducible in the suite |
| NFR-1201 | `wires_the_git_ssh_command_into_the_child` (`tests/cli.rs`) — no user configuration set |
| NFR-1202 | `routes_network_only_to_the_proxy` (`src/profile.rs`) — the profile is unchanged and permits only the proxy |
| NFR-1203 | `builds_a_git_ssh_command_that_names_the_proxy_command_sentinel` (`src/tunnel.rs`) — the value carries no secret, only the sentinel |

## Assumptions

- **sandme's own executable path carries no shell-special character.** The
  `ProxyCommand` value is single-quoted for git's shell layer and re-split on
  spaces by ssh; a space or quote in sandme's install path would break the inner
  layer. Real install paths (`/usr/local/bin/sandme`, a Homebrew or cargo path)
  do not carry them, and the alternative — a second escaping pass across two
  shell layers — is more code than the evidence justifies.
- **A same-uid attacker who could read `GIT_SSH_COMMAND` could already read
  `HTTP_PROXY`.** Inherited from SPEC-0003 Non-goals; the tunnel adds no new
  exposure, and keeping the credential out of the command line (NFR-1203) keeps
  it no more exposed than it already is.

## Open questions

None.

## Implementation tasks

- [x] **T-1201** — `tunnel` module: build the `GIT_SSH_COMMAND` value and act as
      the `ProxyCommand` (open `CONNECT`, relay stdio) (covers FR-1201, FR-1203,
      FR-1204, NFR-1203)
- [x] **T-1202** — Wire `GIT_SSH_COMMAND` into the child in `sandbox.rs`,
      respecting a user-set value and a missing executable path (covers FR-1201,
      FR-1202, FR-1206)
- [x] **T-1203** — Intercept the `ProxyCommand` sentinel in `main` before the
      CLI is parsed; map tunnel failures to the reserved status (covers FR-1203,
      FR-1205)
- [x] **T-1204** — Error variants for the tunnel; reuse the credential module's
      base64 encoder (covers FR-1205, NFR-1203)
- [x] **T-1205** — Tests: unit test for the command builder; integration tests
      for the wiring, the user-set override, and a byte stream through the tunnel
      (covers FR-1201 … NFR-1202)

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-12 | Initial draft, covering issue #33 (split from #29). Adds git-over-SSH tunnelling through the proxy over HTTP CONNECT, reusing SPEC-0003's credential; no Seatbelt profile change. |
