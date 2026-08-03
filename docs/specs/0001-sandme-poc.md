# SPEC-0001: sandme PoC — sandboxed IDE/agent launcher with transparent proxy

## Metadata

- **Status**: Draft
- **Created**: 2026-08-02
- **Updated**: 2026-08-02
- **Related ADRs**: none yet
- **Related specs**: none — this is the baseline spec for the proof of concept.

## Summary

sandme is a CLI tool for macOS users that starts a coding IDE or code agent inside a
sandbox built on the macOS Seatbelt framework, with external network access routed
through a proxy. sandme runs the proxy server in one process and the IDE or code agent
in another, in parallel; the whole arrangement is transparent to the user, who only
types `sandme <command>`.

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

<!-- Derived from the PoC framing; confirm before implementation. -->

- Platforms other than macOS.
- Sandboxing mechanisms other than Seatbelt.
- Inspecting, filtering or blocking traffic at the proxy — this spec only requires that
  traffic be routed through it. Any policy layer is a later spec.
- A GUI, daemon, or persistent service. sandme's lifetime is the lifetime of the
  command it launches.

## User scenarios *(mandatory)*

### Story 1 — Run an IDE or agent sandboxed (P1)

As a macOS developer, I want to launch my IDE or code agent through sandme, so that it
runs confined to a sandbox instead of with my full user privileges.

**Acceptance scenarios**

1. **Given** sandme is installed, **When** the user runs `sandme 'zed ~/Workspace/'`,
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
   launches a proxy server in a background process and the command in a separate
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
   **When** sandme resolves configuration, **Then** the documented precedence applies
   (see FR-009 — currently ambiguous).

### Edge cases

- What happens when the config file at `~/.sandme/config.toml` is absent, unreadable, or
  malformed? [NEEDS CLARIFICATION: fail with an error, or fall back to defaults?]
- What happens when the proxy process fails to start, or exits while the sandboxed
  command is still running? [NEEDS CLARIFICATION]
- What happens to the proxy process when the sandboxed command exits, crashes, or is
  interrupted with Ctrl-C?  [NEEDS CLARIFICATION: cleanup / lifetime coupling]
- What happens when the command passed to sandme does not exist or is not executable?
- What happens when a shared path does not exist, or is a symlink pointing outside the
  shared set?
- How does the system behave for network traffic the proxy cannot handle (non-HTTP
  protocols, DNS, raw sockets)? [NEEDS CLARIFICATION: the proxy is specified as HTTP]

## Requirements *(mandatory)*

### Functional

- **FR-001**: THE SYSTEM SHALL be a CLI binary that receives a command line as input.
- **FR-002**: WHEN invoked with a command line, THE SYSTEM SHALL create a sandboxed
  environment for that command and execute it there.
- **FR-003**: WHEN a sandboxed command runs a subcommand or starts a new process, THE
  SYSTEM SHALL ensure that process also runs under the sandboxed environment.
- **FR-004**: THE SYSTEM SHALL make configured parts of the local filesystem accessible
  to the sandboxed command.
- **FR-005**: WHEN the user invokes sandme, THE SYSTEM SHALL start a proxy server in a
  background process and run the sandboxed command in a separate process, in parallel.
- **FR-006**: THE SYSTEM SHALL automatically route the sandboxed command's network
  traffic through that proxy server, without requiring the user to configure the proxy.
- **FR-007**: THE SYSTEM SHALL read a local configuration file, defaulting to
  `~/.sandme/config.toml`.
- **FR-008**: THE SYSTEM SHALL support configuration through environment variables.
- **FR-009**: WHEN a setting is defined in both the configuration file and the
  environment, THE SYSTEM SHALL apply a documented precedence order.
  [NEEDS CLARIFICATION: the source states both `local config file > environment
  variables` and `env vars override local config` — these contradict each other. Which
  source wins?]

### Non-functional

- **NFR-001**: THE SYSTEM SHALL run on macOS and use the macOS Seatbelt framework as its
  sandboxing mechanism.
- **NFR-002**: THE SYSTEM SHALL be transparent to the user: launching a sandboxed IDE or
  agent requires no steps beyond the single `sandme <command>` invocation and the
  configuration file.
- **NFR-003**: [NEEDS CLARIFICATION: acceptable startup overhead added by sandbox and
  proxy setup — no budget stated.]

## Interface contract

**CLI**

| Flag / argument | Type | Default | Description |
| --- | --- | --- | --- |
| `<command>` | string (positional) | required | The command line to run sandboxed, e.g. `sandme 'zed ~/Workspace/'`. |

[NEEDS CLARIFICATION: no other flags are specified. Candidates implied by the
requirements — a config-path override and a way to declare shared paths on the command
line — are undefined.]

**Configuration** *(file keys and matching environment variables)*

| Key | Env var | Type | Default | Description |
| --- | --- | --- | --- | --- |
| — | — | — | — | [NEEDS CLARIFICATION: the config file path and env-var support are specified, but no concrete keys are. At minimum, shared filesystem paths (FR-004) and proxy settings (FR-005) need keys and an env-var naming scheme.] |

Config file location: `~/.sandme/config.toml` (FR-007).

**Exit codes / errors**

| Code | Condition | Message to user |
| --- | --- | --- |
| — | — | [NEEDS CLARIFICATION: no exit-code contract specified. Open question: does sandme propagate the sandboxed command's exit code, and which codes are reserved for sandme's own failures?] |

## Key entities

- **Sandboxed command**: the user-supplied command line, plus the process tree it spawns.
  All of it runs under one Seatbelt sandbox.
- **Sandbox profile**: the Seatbelt policy applied to the command — derived from the
  shared filesystem paths and the proxy configuration.
- **Shared path**: a location on the local filesystem that the sandboxed command may
  access. Everything not shared is denied.
- **Proxy server**: the HTTP proxy sandme runs in a background process for the lifetime
  of the sandboxed command; the sandbox's external network egress goes through it.
- **Configuration**: the merged result of `~/.sandme/config.toml` and environment
  variables.

## Constraints and dependencies

**Technologies**

- macOS Seatbelt framework
- HTTP proxy server

**Dependencies**

- Rust
- `clap` crate — CLI argument parsing
- `thiserror` crate — error types

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
| FR-001 | CLI parsing unit tests (clap). |
| FR-002 | Integration test: run a command under sandme, assert it executes and is sandboxed. |
| FR-003 | Integration test: sandboxed command spawns a child that attempts a denied access; assert denial. |
| FR-004 | Integration test: read/write inside a shared path succeeds; outside it fails. |
| FR-005 | Integration test: assert two processes (proxy, command) exist while running. |
| FR-006 | Integration test: sandboxed HTTP request is observed at the proxy. |
| FR-007 | Unit test: default path resolution to `~/.sandme/config.toml`. |
| FR-008 | Unit test: env-var-sourced settings are applied. |
| FR-009 | Unit test on precedence — blocked until the contradiction is resolved. |
| NFR-001 | Manual verification on macOS. |
| NFR-002 | Manual walkthrough of the `sandme 'zed ~/Workspace/'` flow. |
| NFR-003 | Startup benchmark — blocked until a budget is set. |

## Assumptions

<!-- Not stated in the original project notes; chosen as reasonable defaults and open to
being overturned. -->

- The proxy is an HTTP proxy, so "network traffic" in FR-006 means HTTP/HTTPS traffic.
  Other protocols are unaddressed.
- The proxy's lifetime is bound to the sandboxed command's lifetime; sandme is not a
  long-running daemon.
- "sandme poc" indicates proof-of-concept scope: correctness of the core mechanism
  matters more than packaging, distribution or performance tuning.

## Open questions

- [ ] Config precedence: does the config file override the environment, or the reverse? — blocks FR-009.
- [ ] Concrete configuration keys and env-var naming scheme — blocks FR-004, FR-007, FR-008.
- [ ] Exit-code contract, including whether the sandboxed command's status is propagated.
- [ ] Behaviour when the config file is missing or malformed.
- [ ] Proxy failure and cleanup semantics: proxy dies mid-run; command exits or is interrupted.
- [ ] Non-HTTP traffic (DNS, raw sockets): denied by the sandbox, or allowed to bypass the proxy?
- [ ] Startup overhead budget — blocks NFR-003.
- [ ] Are shared paths declared in config only, or also via a CLI flag?

## Implementation tasks

<!-- Sequenced but not yet estimated; several are gated on the open questions above. -->

- [ ] **T-001** — CLI skeleton with clap: accept the positional command line; error types with thiserror (covers FR-001)
- [ ] **T-002** — Configuration loading: TOML file at `~/.sandme/config.toml`, env-var support, precedence (covers FR-007, FR-008, FR-009)
- [ ] **T-003** — Seatbelt profile generation from shared paths (covers FR-004)
- [ ] **T-004** — Launch the command under the sandbox, with inheritance to child processes (covers FR-002, FR-003)
- [ ] **T-005** — HTTP proxy server as a background process, run in parallel with the command (covers FR-005)
- [ ] **T-006** — Wire the sandbox's egress to the proxy transparently (covers FR-006, NFR-002)
- [ ] **T-007** — Process lifecycle: startup ordering, shutdown, cleanup on exit or interrupt

## Changelog

| Date | Change |
| --- | --- |
| 2026-08-02 | Converted the original free-form project notes into the spec template. No requirements added or removed; gaps recorded as open questions. |
