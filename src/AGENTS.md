# AGENTS.md — src/

The crate. One binary, one module per responsibility. Each module states its own purpose in its
`//!` doc — read that before editing it; this file only says which module a job belongs to.

## Module map

| Responsibility | Module |
| --- | --- |
| Entry point: parse the invocation, wire the pieces together | `main` |
| Configuration and the environment | `config` |
| Every failure the binary can report | `error` |
| Resolving a command word to a program on disk | `executable` |
| The per-invocation secret that scopes the proxy to sandme's child | `credential` |
| Which network destinations the proxy may reach | `egress` |
| The HTTP forward proxy the child's egress is routed through | `proxy` |
| Git's SSH transport over the proxy's `CONNECT` | `tunnel` |
| Running a command under the platform's sandbox | `sandbox/` |

`sandbox/` holds the platform-independent launch path and one backend per platform: `seatbelt/`
builds an SBPL profile for `sandbox-exec` on macOS, `landlock/` builds a ruleset the child applies
to itself on Linux (SPEC-0015, SPEC-0016). A behaviour that is not platform-specific belongs above
the backends, not duplicated inside both.

## Rules that span the tree

- Errors live in the one `thiserror` enum in `error`. A module does not define its own error type.
- Only `config` reads configuration files and the environment. Every other module takes what it
  needs as an argument. This is what makes the rest of the crate testable without a machine.
- The Seatbelt profile is **last-match-wins**. The closing denials in `sandbox/seatbelt/profile.rs`
  must stay last: a rule emitted after them overrides them and opens the sandbox, with every test
  still green. Add grants before the closing section, never after it.

## Where to go deeper

`docs/guidelines/code/` is the authority on structure, naming, docs, errors and the quality gate;
`docs/guidelines/architecture/` on the CLI surface; `docs/guidelines/performance/` on the startup
path and async. Read the guideline rather than this file — an AGENTS.md MUST NOT restate one.

Changing `sandbox/seatbelt/profile.rs`, `proxy`, `egress` or how the child is launched triggers
`/security-audit` before the PR. The root `AGENTS.md` carries the full review table.
