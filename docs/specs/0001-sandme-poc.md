# SPEC-0001: sandme PoC — sandboxed IDE/agent launcher with transparent proxy

## Metadata

- **Status**: Accepted
- **Created**: 2026-08-02
- **Updated**: 2026-08-08
- **Related ADRs**: none yet
- **Related specs**: none — this is the baseline spec for the proof of concept.

## Summary

sandme is a CLI tool for macOS users that starts a coding IDE or code agent inside a
sandbox built on the macOS Seatbelt framework, with external network access routed
through a proxy. sandme runs the proxy server as a concurrent task it manages and the
IDE or code agent in a separate process, in parallel; the whole arrangement is
transparent to the user, who only types `sandme <command>`.

## Problem

Coding IDEs and code agents run with the full privileges of the user who launched them:
unrestricted filesystem access and unrestricted network egress. A user who wants to run
an agent against one project has no lightweight way to confine what it can read, write,
or reach. Existing options (VMs, containers) are heavyweight on macOS and break the
native desktop experience of an IDE.

## Goals

- Launch an arbitrary command inside a Seatbelt sandbox from a single CLI invocation.
- Confine the sandboxed command's filesystem access to explicitly shared paths.
- Route the sandboxed command's network traffic through a sandme-managed proxy.
- Require no manual setup from the user beyond configuration — the sandbox and the
  proxy are wired up transparently.

## Non-goals

- Platforms other than macOS.
- Sandboxing mechanisms other than Seatbelt.
- Inspecting, filtering or blocking traffic at the proxy — this spec only requires that
  traffic be routed through it. Any policy layer is a later spec.
- A GUI, daemon, or persistent service. sandme's lifetime is the lifetime of the
  command it launches.
- Shell interpretation of multi-argument commands: when the user passes
  multiple operands (`sandme ls -ltra ./`), they reach the command unchanged.
  A single quoted operand (`sandme 'ls | grep foo'`) is routed through
  `/bin/sh -c` so shell features work inside it.

## User scenarios *(mandatory)*

### Story 1 — Run an IDE or agent sandboxed (P1)

As a macOS developer, I want to launch my IDE or code agent through sandme, so that it
runs confined to a sandbox instead of with my full user privileges.

**Acceptance scenarios**

1. **Given** sandme is installed, **When** the user runs `sandme zed ~/Workspace/`,
   **Then** Zed starts and runs inside a Seatbelt sandbox.
2. **Given** a sandboxed command is running, **When** it invokes a subcommand or spawns
   a new process, **Then** that child process also runs under the same sandbox.
3. **Given** a sandboxed command is running, **When** it attempts to access a filesystem
   path that was not shared, **Then** the access is denied by the sandbox.

### Story 2 — Network access is proxied automatically (P1)

As a macOS developer, I want the sandboxed command's external network access to go
through sandme's proxy without my configuring anything, so that egress is mediated by
default.

**Acceptance scenarios**

1. **Given** the user runs `sandme <command>`, **When** sandme starts, **Then** it
   starts the proxy server as a concurrent task and the command in a separate
   process, in parallel.
2. **Given** a sandboxed command makes an external network request, **When** the request
   leaves the sandbox, **Then** it is routed through the sandme proxy without the user
   having configured proxy settings manually.

### Story 3 — Share selected filesystem paths (P2)

As a macOS developer, I want to declare which parts of my local filesystem the sandboxed
command may use, so that it can work on my project without seeing the rest of my disk.

**Acceptance scenarios**

1. **Given** a path is declared as shared, **When** the sandboxed command accesses it,
   **Then** the access succeeds.
2. **Given** a path is not declared as shared, **When** the sandboxed command accesses
   it, **Then** the access is denied.

### Story 4 — Configure via file and environment (P3)

As a macOS developer, I want sandme's behaviour to come from a config file with
environment-variable support, so that I can keep stable settings on disk and override
them per invocation.

**Acceptance scenarios**

1. **Given** no `--config` is supplied, **When** sandme starts, **Then** it reads
   `~/.sandme/config.toml`.
2. **Given** a setting is present in both the config file and the environment,
   **When** sandme resolves configuration, **Then** the environment wins (FR-009).

### Edge cases

- Config file absent → defaults are used. Unreadable or malformed → sandme reports the
  cause and exits 1.
- Proxy fails to start → the whole invocation fails (a sandbox without its proxy is a
  broken sandbox). Per-request forwarding failures answer `502` to the client; they do
  not abort the invocation.
- The sandboxed command exits, crashes, or is interrupted with Ctrl-C → the proxy stops
  and sandme exits; nothing outlives the command.
- The command does not exist or is not executable → the sandbox layer reports
  it and sandme propagates that status.
- A shared path does not exist → the sandbox simply never grants it. A shared path that
  is a symlink is resolved first; the resolved target is what gets shared.
- Network traffic the proxy cannot handle (non-HTTP protocols, DNS, raw sockets) is
  denied by the sandbox: the profile allows TCP to the proxy port only.

## Requirements *(mandatory)*

### Functional

- **FR-001**: THE SYSTEM SHALL be a CLI binary that receives a command line as input.
- **FR-002**: WHEN invoked with a command line, THE SYSTEM SHALL create a sandboxed
  environment for that command and execute it there.
- **FR-003**: WHEN a sandboxed command runs a subcommand or starts a new process, THE
  SYSTEM SHALL ensure that process also runs under the sandboxed environment.
- **FR-004**: THE SYSTEM SHALL make configured parts of the local filesystem accessible
  to the sandboxed command.
- **FR-005**: WHEN the user invokes sandme, THE SYSTEM SHALL start a proxy server as a
  concurrent task and run the sandboxed command in a separate process, in parallel.
- **FR-006**: THE SYSTEM SHALL automatically route the sandboxed command's network
  traffic through that proxy server, without requiring the user to configure the proxy.
  To that end sandme sets `HTTP_PROXY`/`HTTPS_PROXY` (upper- and lowercase) in the
  child's environment, overriding any value already present: FR-006 cannot hold if the
  child may bypass the proxy (the override is recorded here per posix.md §6).
- **FR-007**: THE SYSTEM SHALL read a local configuration file, defaulting to
  `~/.sandme/config.toml`.
- **FR-008**: THE SYSTEM SHALL support configuration through environment variables.
- **FR-009**: WHEN a setting is defined in both the configuration file and the
  environment, THE SYSTEM SHALL let the environment win.

### Non-functional

- **NFR-001**: THE SYSTEM SHALL run on macOS and use the macOS Seatbelt framework as its
  sandboxing mechanism.
- **NFR-002**: THE SYSTEM SHALL be transparent to the user: launching a sandboxed IDE or
  agent requires no steps beyond the single `sandme <command>` invocation and the
  configuration file.
- **NFR-003**: *(withdrawn)* — no startup-overhead budget was stated, and the PoC
  assumptions put correctness above performance tuning. A budget belongs to a later
  performance work item.

## Interface contract

**CLI**

| Flag / argument | Type | Default | Description |
| --- | --- | --- | --- |
| `<command> [args...]` | operands | required | The command to run sandboxed. When multiple operands are given (`sandme zed ~/Workspace/`), they are passed to the child unshelled and unquoted — the child receives its own argument vector. When a single operand is given (`sandme 'ls \| grep foo'`), it is routed through `/bin/sh -c` so shell features (pipes, redirections, globbing) work inside it. `--` terminates sandme's option parsing, so the child's own options reach it: `sandme -- curl -sS https://example.com/`. |

The PoC exposes no other flags; shared paths are configured through the config file or
environment only.

**Configuration** *(file keys and matching environment variables)*

| Key | Env var | Type | Default | Description |
| --- | --- | --- | --- | --- |
| `shared_paths` | `SANDME_SHARED_PATHS` | array of strings (env: comma-separated) | `["~/"]` | Filesystem paths the sandboxed command may read and write. `~/` prefixes expand to the home directory; symlinks are resolved. |
| `proxy_port` | `SANDME_PROXY_PORT` | integer (u16) | `8787` | Loopback port the egress proxy listens on. |

Config file location: `~/.sandme/config.toml` (FR-007).

**Exit codes / errors**

| Code | Condition | Message to user |
| --- | --- | --- |
| `0` | The sandboxed command exited `0`. | — |
| `1`–`125` | The sandboxed command's own exit code, propagated unchanged — including statuses produced by the sandbox layer itself, e.g. `71` when the command is not found. `1` is also sandme's own failure code (unreadable or malformed config, proxy could not bind its port, command could not be started). | A message naming the cause (and a hint where one exists). |
| `128+n` | The sandboxed command was terminated by signal `n` (a Ctrl-C child reports `130`). | — |

## Key entities

- **Sandboxed command**: the user-supplied command line, plus the process tree it spawns.
  All of it runs under one Seatbelt sandbox.
- **Sandbox profile**: the Seatbelt policy applied to the command — derived from the
  shared filesystem paths and the proxy configuration.
- **Shared path**: a location on the local filesystem the sandboxed command may
  access. Everything not shared is denied.
- **Proxy server**: the HTTP proxy sandme runs as a concurrent task for the lifetime of
  the sandboxed command; the sandbox's external network egress goes through it.
- **Configuration**: the merged result of `~/.sandme/config.toml` and environment
  variables.

## Constraints and dependencies

**Technologies**

- macOS Seatbelt framework (SBPL profiles via `sandbox-exec`)
- HTTP proxy server

**Dependencies**

- Rust
- `clap` — CLI argument parsing
- `serde`, `toml` — configuration
- `thiserror` — error types
- `hyper`, `hyper-util`, `tokio` — proxy server and process lifecycle

## Success criteria *(mandatory)*

- **SC-001**: A user can start their IDE or code agent sandboxed with a single
  `sandme <command>` invocation and no manual sandbox or proxy setup.
- **SC-002**: A process running under sandme — including any subprocess it spawns —
  cannot read or write filesystem locations outside the shared paths.
- **SC-003**: External network traffic originating from the sandboxed process is
  observed at the sandme proxy.
- **SC-004**: The sandboxed IDE or agent remains usable for its normal workflow on the
  shared project directory.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-001 | Integration tests in `tests/cli.rs` — the built binary driven through its CLI, including a multi-word operand passed to the child unchanged (`passes_multi_word_arguments_unchanged`). |
| FR-002 | `prints_child_output`, `propagates_child_exit_status`-equivalent propagation and `reports_signal_deaths_as_128_plus_n` (`tests/cli.rs`). |
| FR-003 | Seatbelt applies the profile to the whole process tree (`process-exec` without `no-sandbox`); verified manually: a sandboxed shell launching `curl` gets its non-proxy egress denied. |
| FR-004 | `denies_writes_outside_shared_paths` (`tests/cli.rs`) plus manual read-denial verification. |
| FR-005 | Manual: proxy task and sandboxed command alive concurrently; proxy stops when the command exits. |
| FR-006 | `routes_http_egress_through_the_proxy` (`tests/cli.rs`) plus manual verification that direct egress bypassing the proxy is denied and HTTPS works via CONNECT. |
| FR-007 | `keeps_defaults_for_absent_settings` (unit); integration tests isolate `HOME` to prove default path resolution. |
| FR-008 | `environment_overrides_previous_values` (unit, `src/config.rs`). |
| FR-009 | Same test — environment values replace file values. |
| NFR-001 | Developed and verified on macOS 26 with Seatbelt via `sandbox-exec`. |
| NFR-002 | Manual walkthrough of the `echo`/`curl` flows: no setup beyond the invocation. |
| NFR-003 | Withdrawn. |

## Assumptions

<!-- Not stated in the original project notes; chosen as reasonable defaults and open to
being overturned. -->

- The proxy is an HTTP proxy, so "network traffic" in FR-006 means HTTP/HTTPS traffic.
  Other protocols are denied by the sandbox.
- The proxy's lifetime is bound to the sandboxed command's lifetime; sandme is not a
  long-running daemon.
- "sandme poc" indicates proof-of-concept scope: correctness of the core mechanism
  matters more than packaging, distribution or performance tuning.

## Open questions

None — all resolved; see the 2026-08-08 changelog entry.

## Implementation tasks

- [x] **T-001** — CLI skeleton with clap: accept the positional command line; error types with thiserror (covers FR-001)
- [x] **T-002** — Configuration loading: TOML file at `~/.sandme/config.toml`, env-var support, precedence (covers FR-007, FR-008, FR-009)
- [x] **T-003** — Seatbelt profile generation from shared paths (covers FR-004)
- [x] **T-004** — Launch the command under the sandbox, with inheritance to child processes (covers FR-002, FR-003)
- [x] **T-005** — HTTP proxy server as a concurrent task, run in parallel with the command (covers FR-005)
- [x] **T-006** — Wire the sandbox's egress to the proxy transparently (covers FR-006, NFR-002)
- [x] **T-007** — Process lifecycle: startup ordering, shutdown, cleanup on exit or interrupt

## Changelog

| Date | Change |
| --- | --- |
| 2026-08-02 | Converted the original free-form project notes into the spec template. No requirements added or removed; gaps recorded as open questions. |
| 2026-08-08 | PoC implemented (T-001…T-007); status → Accepted. All open questions resolved in the body; FR-005 records the proxy as a concurrent task; NFR-003 withdrawn. |
| 2026-08-08 | Interface aligned with `docs/guidelines/architecture/posix.md`: the command is operands, not a quoted string (`shell-words` removed); env vars use the `SANDME_` prefix; signal deaths exit `128+n`; stdout carries only the child's output; the profile reaches `sandbox-exec` via `-p`; the config file is parsed with serde and malformed input is reported. |
| 2026-08-08 | Single-operand commands (`sandme 'ls \| grep foo'`) are routed through `/bin/sh -c` so shell features (pipes, redirections, globbing) work. Multi-operand commands remain direct-exec. See [issue #6](https://github.com/leopepe/sandme/issues/6). |
