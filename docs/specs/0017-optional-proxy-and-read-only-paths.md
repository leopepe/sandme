# SPEC-0017: Optional proxy and read-only paths

## Metadata

- **Status**: Draft
- **Created**: 2026-09-13
- **Updated**: 2026-09-13
- **Related ADRs**: none yet
- **Related specs**: SPEC-0001 (qualifies FR-005/FR-006 and its `proxy_port` default),
  SPEC-0003 (its egress FRs hold only while the proxy is on), SPEC-0014 (qualifies its `proxy_port`
  default; builds on FR-1404), SPEC-0016 (clarifies FR-1604 and preserves NFR-1601)

## Summary

Makes the egress proxy optional, flips the default `proxy_port` to `0` (the OS-chosen ephemeral
port) so parallel runs never collide, and adds a `read_only_paths` grant that widens reads without
widening writes. Each is opt-in; the default invocation — proxy on, empty `read_only_paths` — is
byte-for-byte unchanged. This spec records the requirement-level deltas these changes make to the
Implemented specs, without editing those specs in place (their requirement IDs are immutable).

## Problem

Running many `sandme` invocations in parallel is awkward. Every invocation stands up an egress
proxy on a fixed default port (`8787`), so two concurrent runs collide on the port, and a run that
needs no network still pays for a proxy it does not want. Separately, the sandbox grants read access
only to a fixed base set plus the read-write `shared_paths`: a caller who wants a toolchain
directory (a Homebrew prefix, a language runtime) readable-but-not-writable has no way to say so
without also granting write.

## Goals

- The egress proxy can be turned off; when off, the command gets no egress on macOS, and no outbound
  TCP on Linux (UDP and raw sockets stay unrestricted, a pre-existing Landlock limitation consistent
  with SPEC-0016's TCP-only egress model).
- Parallel proxied runs are collision-free with no per-invocation configuration.
- A caller can widen reads (and execute) without widening writes.
- The default invocation stays byte-for-byte identical to before this change.

## Non-goals

- No `--no-proxy` CLI flag or any other new command-line flag (SPEC-0001: config/env only).
- No new proxy *modes* (transparent, external); `proxy` is a single boolean.
- No read-without-execute bucket; `read_only_paths` is one bucket, read+execute (design D5).
- No reconciliation machinery for `proxy = false` + `allow_private_egress` beyond documentation.
- No baking a toolchain directory (e.g. `/opt/homebrew`) into the base set.
- No re-doing the IPv6 ephemeral-port fix; no CI matrix (the Linux path stays a review claim,
  executed by the deferred #56 CI).

## Spec deltas

### ADDED

- **FR-1701** … **FR-1706** below (optional proxy, port-0 default, read-only paths, widening
  provenance, deny-all-TCP-is-fully-enforced, default-invocation-unchanged).

### MODIFIED

- **SPEC-0001/FR-005 and FR-006 — the proxy is unconditional** — were: sandme *always* starts a
  proxy (FR-005) and *always* routes the command's traffic through it (FR-006). Now conditional on
  `proxy = true` (the default). WHILE `proxy` is `true`, FR-005 and FR-006 hold exactly as written.
  WHEN `proxy` is `false`, they are suspended for that invocation: no proxy is started, no
  `HTTP(S)_PROXY` is set, and the command is granted no egress (FR-1701). Reason: the batch use case
  needs a no-network run to be expressible, which is strictly more restrictive than FR-005/FR-006.

- **SPEC-0001/Interface contract — the `proxy_port` default** — was: `proxy_port` defaults to
  `8787`. Now defaults to `0`, the OS-chosen ephemeral port (FR-1702), which makes concurrent runs
  collision-free. An explicit non-zero `proxy_port` still binds that exact port.

- **SPEC-0014/Interface contract — the `proxy_port` default row** — was: the config table records
  `8787` as the default. Now `0`, consistent with the flip above. SPEC-0014/FR-1404 already
  specifies and tests the port-0 behaviour (the OS picks a free port, read back and pinned into the
  grant); this delta only changes the *documented default* to that already-supported value, so it is
  safe by construction.

- **SPEC-0003 — egress restrictions are conditional on the proxy** — the egress FRs (FR-201 the
  destination denials, FR-203 the per-run credential, FR-205 `allow_private_egress`, FR-206 the DNS
  resolution) describe what the proxy relays. They continue to hold in full WHILE `proxy` is `true`.
  WHEN `proxy` is `false` there is no relay, so they are moot for that invocation — nothing
  misbehaves. In particular `allow_private_egress` is inert when `proxy = false` (it only affects
  what the relay forwards); if it is env-sourced it still warns via its existing provenance path,
  which is harmless. No SPEC-0003 requirement is weakened while the proxy is on.

- **SPEC-0016/FR-1604 — an empty allowed-port set is FULLY ENFORCED, not unenforced** — FR-1604
  requires the kernel to enforce *both* the filesystem restriction and the TCP-port egress
  restriction, refusing to run below ABI v4. This delta clarifies (it does not weaken FR-1604): when
  `proxy` is `false`, the Landlock ruleset presents an **empty** allowed-port set while still calling
  `handle_access(ConnectTcp)` — a deny-all outbound TCP rule. This is a *fully-enforced* egress
  state. The enforceability gate SHALL treat it as fully enforced and run the command confined; it
  SHALL NOT read "no proxy port" as "egress unenforced" and refuse. The kernel feature floor is
  unchanged: enforcing "no outbound TCP" still *uses* the `ConnectTcp` access type, which arrives at
  v4, so the floor stays v4 — dropping to v1 (filesystem only) would leave outbound TCP
  unrestricted, the opposite of what proxy-off wants. This is recorded so a future reader does not
  "optimize" the floor down when the proxy is off.

- **SPEC-0016/FR-1605 — the read set gains `read_only_paths`** — FR-1605 confines filesystem access
  to an enumerated allowlist (read+execute on system dirs, read-only on config/cert/random paths,
  read-write on `shared_paths`). This delta adds `read_only_paths` to that allowlist: each entry is
  granted read+execute (never write), resolved and guarded through the *same* twice-checked
  `guard_shared_path` (lexical string, then canonicalized path) as `shared_paths`, so a `/` or
  `/proc`/`/sys` read entry is refused exactly as a write entry would be (FR-1607/issue #31 stays
  closed). NFR-1601 is preserved: an empty `read_only_paths` adds nothing.

## Requirements *(mandatory)*

### Functional

- **FR-1701**: THE SYSTEM SHALL expose a boolean configuration key `proxy`, defaulting to `true`,
  with matching environment variable `SANDME_PROXY` following the house boolean convention (`1` or
  `true`, case-insensitive, enables; the environment wins over the file, FR-009). WHILE `proxy` is
  `true` THE SYSTEM SHALL behave exactly as before this change (start the proxy, route the command
  through it, grant egress to the proxy). WHEN `proxy` is `false` THE SYSTEM SHALL start no proxy,
  set none of `HTTP_PROXY`/`HTTPS_PROXY`/`http_proxy`/`https_proxy` in the child environment, wire no
  git-over-SSH `ProxyCommand`, and grant the command no network egress — on macOS no outbound access
  of any kind, and on Linux no outbound TCP (UDP and raw sockets stay unrestricted, a pre-existing
  Landlock limitation consistent with SPEC-0016's TCP-only egress model). There SHALL be no
  command-line flag for this.

- **FR-1702**: THE SYSTEM SHALL default `proxy_port` to `0`, the OS-chosen ephemeral port
  (SPEC-0014/FR-1404). The bound port SHALL be read back from the listener and pinned into the
  sandbox grant. An explicit non-zero `proxy_port` SHALL still bind that exact port.

- **FR-1703**: THE SYSTEM SHALL expose a configuration key `read_only_paths`, an array of strings
  defaulting to **empty**, with matching environment variable `SANDME_READ_ONLY_PATHS`
  (comma-separated, trimmed, empties filtered; the environment wins, FR-009). Each entry SHALL have a
  leading `~/` expanded and symlinks resolved, and SHALL be granted read **and execute** access
  **without** write, in addition to the base grants and `shared_paths`. An empty `read_only_paths`
  SHALL add no grant. THE SYSTEM SHALL NOT bake any toolchain directory into the base set.

- **FR-1704**: IF a `read_only_paths` entry is `/`, or resolves through a symlink onto a protected
  pseudo-filesystem (`/proc`, `/sys`), THEN THE SYSTEM SHALL refuse the invocation
  (`SandmeError::SharedPathTooBroad`), reusing the same guard as `shared_paths`, rather than grant a
  tree that would re-open another process's environment (issue #31).

- **FR-1705**: THE SYSTEM SHALL treat `read_only_paths` as a security-widening setting for the
  issue-#30 provenance warnings: WHEN the environment (`SANDME_READ_ONLY_PATHS`), rather than the
  config file or the default, broadens `read_only_paths` beyond its file-or-default baseline, THE
  SYSTEM SHALL emit a warning on stderr, consistent with `shared_paths`. A value set in the config
  file or left at its default SHALL NOT warn. `proxy` is a *narrowing* boolean and SHALL NOT warn.

- **FR-1706**: WHERE the platform sandbox enforces outbound TCP by an allow-list of ports (Linux),
  WHEN `proxy` is `false` THE SYSTEM SHALL present an empty allowed-port set while still handling the
  outbound-TCP access type (deny-all outbound TCP). The enforceability gate SHALL treat this as
  **fully enforced** and run the command confined; it SHALL NOT interpret the absence of a
  proxy-port grant as "egress unenforced" and refuse. The kernel feature floor (ABI v4) SHALL be
  unchanged by disabling the proxy.

### Non-functional

- **NFR-1701**: THE SYSTEM SHALL keep the default invocation — `proxy` enabled and `read_only_paths`
  empty — byte-for-byte identical to its behaviour before this change: the macOS Seatbelt profile is
  generated identically (SPEC-0016/NFR-1601) and the Linux Landlock ruleset is built identically.
  The new grants and the proxy omission SHALL only append to, or (for the proxy) remove from, the
  generated policy, never reorder the unchanged parts.

## Constraints and dependencies

- Development is macOS-only this cycle; the Landlock edits are `cfg(target_os = "linux")` and are a
  write-and-review claim, executed by the deferred #56 CI. The `plan.rs` decision layer is
  `cfg`-neutral and unit-tested on macOS.
- `proxy = false` disables **all** network remotes — git-over-SSH (the gated `wire_git_ssh` tunnel,
  design D1) **and** HTTP(S) — not just the HTTP proxy. An SSH git failure under `proxy = false` is
  expected behaviour, not a bug.

## Success criteria *(mandatory)*

- **SC-1701**: With `proxy = false`, a sandboxed command has no `HTTP(S)_PROXY` set and cannot reach
  the network; no proxy process is running.
- **SC-1702**: Two concurrent default invocations bind different OS-chosen ports and neither fails on
  a port collision.
- **SC-1703**: A `read_only_paths` entry is readable and executable inside the sandbox but not
  writable; an empty `read_only_paths` leaves the generated policy unchanged.
- **SC-1704**: The macOS SBPL for the default invocation is byte-for-byte identical to the
  pre-change profile.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-1701 | `config::tests::disables_the_proxy_when_the_environment_asks_for_it`, `enables_the_proxy_for_a_true_ish_spelling`; `seatbelt::profile::tests::omits_the_egress_line_when_the_proxy_is_disabled`; `tests/cli.rs` no-proxy egress-denied case |
| FR-1702 | `config::tests::keeps_defaults_for_absent_settings` (`proxy_port == 0`); SPEC-0014/FR-1404 tests (`routes_http_egress_through_the_proxy`, whole suite on port 0) |
| FR-1703 | `seatbelt::profile::tests::grants_a_read_only_path_read_without_write`, `empty_read_only_paths_add_nothing`; `landlock::plan::tests::grants_read_only_paths_read_execute_never_read_write`; `tests/cli.rs` read-only case |
| FR-1704 | `landlock::plan::tests::rejects_a_shared_path_that_would_re_admit_a_protected_tree` (the guard `read_only_paths` reuses) — REVIEW |
| FR-1705 | `config::tests::warns_when_the_environment_broadens_read_only_paths`, `stays_silent_when_the_config_file_sets_read_only_paths` |
| FR-1706 | `landlock::plan::tests::grants_no_outbound_tcp_when_the_proxy_is_off`; REVIEW that `enforce()`/`enforce_abi_floor()` are unchanged — executed by the deferred #56 CI |
| NFR-1701 | `seatbelt::profile::tests::the_default_proxy_on_profile_is_byte_for_byte_unchanged` |

## Assumptions

- Execute on a pure-data read-only directory is harmless (nothing to exec), so a single read+execute
  bucket is the simplest superset serving the toolchain case (design D5).
- A user who relied on the fixed `8787` sets `proxy_port = 8787` explicitly to restore it.

## Open questions

- None.

## Implementation tasks

See `openspec/changes/batch-execution-support/tasks.md` (this spec is authored by task 4.1 of that
change).

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-13 | Initial draft: records the optional-proxy, port-0-default, read-only-paths, widening-provenance, and deny-all-TCP deltas. |
