# ADR-0001: Separate proxy-server and SSH-client modules with declarative facades

## Metadata

- **Status**: Accepted
- **Created**: 2026-10-22
- **Updated**: 2026-10-23
- **Related specs**: SPEC-0018
- **Supersedes**: none

## Context

At source base `fccd5d7db3b2afb6ec399b3428e615a20ff619d7`, `src/proxy.rs` contains
442 lines, including tests. It owns the running-server handle, listeners, connection
handling, request authentication, destination dispatch, HTTP forwarding and server-side
CONNECT upgrades. `src/egress.rs` is consumed only by that request path.

`src/tunnel.rs` is different: it builds git's SSH `ProxyCommand` and implements that
command's HTTP CONNECT **client**, running inside the sandbox. Its command builder and
runner share the sentinel and argument contract. Sharing a wire protocol does not make
this client an implementation detail of the server.

The owner selected sibling directory-backed modules and explicitly requested that their
`mod.rs` files declare components and expose the crate-facing interface rather than
contain implementation. This is a convention for the two modules in this change, not a
claim that Rust prohibits logic in `mod.rs` or that the existing sandbox facade follows it.
The owner explicitly approved this written record on 2026-10-23 in the Conductor implementation session.

## Decision

### D1 — Two sibling responsibilities

Use `src/proxy/` for the HTTP proxy service and `src/tunnel/` for the SSH CONNECT client.
Do not nest the client below the server or introduce a common networking parent.

The implementation destinations are:

| Destination | Responsibility |
| --- | --- |
| `src/proxy/mod.rs` | Proxy module documentation, child declarations and crate-facing re-exports |
| `src/proxy/server.rs` | `Server`, listener startup, accept loops and per-connection HTTP service setup |
| `src/proxy/request.rs` | Authenticated request processing: destination extraction, egress decision, HTTP forwarding, CONNECT upgrade and responses |
| `src/proxy/egress.rs` | Existing destination resolution and restricted-range classification |
| `src/tunnel/mod.rs` | Tunnel module documentation, child declaration and crate-facing re-exports |
| `src/tunnel/client.rs` | Existing SSH command builder, sentinel, CONNECT exchange and stdio relay |

This is the destination decision for this refactor, not a second maintained index of the
whole source tree. The module declarations and module-purpose rustdoc remain its source map.

### D2 — Declarative entry files

The two `mod.rs` files contain module-purpose rustdoc, necessary module attributes, child
module declarations and explicit re-exports only. No function bodies, forwarding wrappers,
constants, type definitions, implementations or test bodies belong in those files.

The proxy facade declares private `server`, `request` and `egress` children and re-exports
`Server` and `serve` with `pub(crate)` visibility. The tunnel facade declares private
`client` and re-exports `PROXY_COMMAND_ARG`, `git_ssh_command` and `run` with `pub(crate)`
visibility. Do not expose the children themselves or use wildcard exports.

### D3 — Keep the security decision chain together

`request.rs` owns `Body`, `Policy`, `Policy::authorizes`, `route`, `destination_of`, `refuse`,
`unauthorized`, `forward`, `to_origin_form`, `strip_hop_by_hop`, `hop_by_hop_headers`,
`open_tunnel` and `response_with` from the current proxy implementation.

`Policy`, its two construction fields, `Body` and `route` may be `pub(super)` where needed
by `server.rs`; they are not crate-facing API. Egress items used by the request sibling
may likewise be `pub(super)`. Keep all other implementation items private.

Preserve the existing order and concrete types: credential check, destination extraction,
one vetted resolution, then forwarding or server-side CONNECT. Do not introduce a new
policy module, trait, resolver abstraction, connector, HTTP client or task supervisor.
Server-side `open_tunnel` stays in `request.rs`; it is not the SSH client in `tunnel/`.

### D4 — Keep shared credential code in its existing home

Leave `src/credential.rs` at the crate root. The server owns a `Credential`; the SSH client
reuses only its Base64 encoder when reading the credential from the proxy URL. Do not move
this shared code into a private server child, duplicate the encoder, or introduce a general
authentication/codec layer for this refactor.

`config`, `error`, `executable`, child environment wiring and the sandbox backends keep their
current ownership. In particular, `src/sandbox/mod.rs` and all sandbox backend sources are
not implementation targets. The existing `proxy::Server` and `tunnel::*` paths keep those
callers unchanged. Remove only the now-obsolete root `mod egress;` declaration from `main`.

### D5 — Relocate tests according to their subject

Keep destination parsing tests in `request.rs`, range tests in `egress.rs`, and the command
builder test in `client.rs`, preserving their leaf names and assertions.

The existing `serves_both_loopback_families_on_the_published_port` test crosses a live socket
boundary. Move its behavior to `tests/cli.rs`, driving the binary with an ephemeral port;
retain its nonzero-port and same-published-port IPv4/IPv6 assertions. Do not populate the
facades with dummy tests. Add only the bounded SSH failure characterizations identified in
SPEC-0018's task T-1803.

### D6 — Preserve active traceability without rewriting history

Update active source pointers in verification tables and review guidance after relocation.
Do not edit existing requirement text, historical problem statements, completed task
narratives, archived OpenSpec changes or past audit reports merely because they name an old
path. Source-location-only verification updates get a Changelog note, not a new requirement
or a Status change in an older spec.

The security-sensitive set must cover the entire new `src/proxy/` subtree; retain every
other previously covered path. Treat the SSH helper as security-sensitive for this cycle
as well. Relocation must not make Conductor or repository reviewers miss the code.

## Consequences

- Readers can inspect either facade without reading transport implementation; callers keep
  their existing paths. No library target, dependency, flag or configuration surface is added.
- `proxy.rs` is split at an existing lifecycle/request seam rather than merely renamed to a
  similarly large `mod.rs`. The tunnel stays cohesive in one implementation file.
- A reader makes one additional navigation hop into `client.rs`. That is the deliberate cost
  of the owner's facade convention, not justification for further subdivision.
- `Policy` and a few existing request/egress items gain sibling visibility, but not visibility
  outside `proxy/`.
- `credential.rs` remains shared at the root. This is a narrow existing dependency, not an
  abstraction extracted for possible future clients.
- Existing sandbox entry-file logic is intentionally untouched. A repository-wide facade
  conversion would be a different change.

Alternatives considered: nesting `tunnel` under `proxy` obscures client/server ownership;
putting all tunnel logic in `mod.rs` violates the selected facade convention; keeping only
flat files misses the requested component organization; a generic networking/authentication
layer adds responsibilities that no current requirement needs.

## Changelog

| Date | Change |
| --- | --- |
| 2026-10-23 | Owner explicitly accepted ADR-0001 together with SPEC-0018; D1–D6 are binding. Implementation and verification remain pending. |
