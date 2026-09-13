# SPEC-0015: Sandbox backend abstraction

## Metadata

- **Status**: Draft
- **Created**: 2026-09-13
- **Updated**: 2026-09-13
- **Related ADRs**: none yet
- **Related specs**: SPEC-0001 (extends; refactors how the sandbox is applied), SPEC-0013 (the
  single-operand shell routing, now a shared helper)

## Summary

`sandme` applies its sandbox one way: build an SBPL text profile and hand it to macOS
`sandbox-exec`. That is baked into `main` and `sandbox::run`, so a second platform has nowhere to
plug in. This spec introduces a `Backend` trait and moves the macOS implementation behind it, so a
Linux/Landlock backend ([#8](https://github.com/leopepe/sandme/issues/8)) can be added later as a
sibling without touching the shared orchestration. **This change is the abstraction only** — no
Linux code, no CI, no behaviour change on macOS.

## Spec deltas

### MODIFIED

- **SPEC-0001**, how the sandbox is applied. Was: `main` builds an SBPL profile
  (`generate_profile`) and calls `sandbox::run(profile, proxy, command)`, which invokes
  `sandbox-exec`. Now: `sandbox::run(config, proxy, command)` selects a `Backend` (by
  `cfg(target_os)`) that owns its own restriction translation; SBPL generation and the app-bundle
  redirect become the macOS (`seatbelt`) backend's internals. No observable behaviour changes.

## Problem

The codebase assumes one sandboxing mechanism:

- `main::run` calls `profile::generate_profile` (SBPL) then `sandbox::run(&profile, …)`.
- `sandbox::run` shells out to `sandbox-exec -p <profile>`.
- SBPL syntax, the `.app` redirect and the `sandbox-exec` exit-status convention (`EX_OSERR = 71`)
  are all macOS-specific, and none sits behind an interface.

Landlock builds a kernel ruleset programmatically from the config — there is no text profile — so
it cannot reuse `generate_profile` and has nowhere to attach under the current shape.

## Goals

- One trait that both a macOS and a future Linux backend satisfy, selected at compile time.
- The cross-platform orchestration (operand→program/args shell routing, `HTTP_PROXY`/
  `GIT_SSH_COMMAND` wiring, Ctrl-C, exit-status propagation) lives once, in the shared module.
- Everything macOS-specific (SBPL, the `.app` redirect, the `71` decode) lives in the `seatbelt`
  backend.
- Zero behaviour change on macOS, proven by the existing integration suite passing unchanged.

## Non-goals

- The Landlock backend itself, a `landlock.rs`, any `cfg(target_os = "linux")` code, or the
  `landlock` crate. Those are the #8 follow-up, which this seam exists to receive.
- CI, and any change to SPEC-0001/NFR-001's "macOS only" — the platform is still macOS-only until
  the Linux backend lands.
- Runtime backend selection or a `dyn Backend`: exactly one backend compiles per target, so
  dispatch is a `cfg(target_os)` type alias, static, no trait object.
- Relocating the platform-agnostic modules (`proxy`, `egress`, `credential`, `tunnel`, `config`).

## User scenarios *(mandatory)*

### Story 1 — Nothing changes for a macOS user (P1)

As a macOS user, I want the refactor to be invisible: every command behaves exactly as before.

**Acceptance scenarios**

1. **Given** any command that worked before the refactor, **When** it runs under sandme, **Then**
   its output, exit status, filesystem confinement and egress are identical.

### Story 2 — A new backend has one place to attach (P1)

As a contributor adding Linux support, I want a single trait to implement and a single `cfg` arm to
add, without touching the shared orchestration.

**Acceptance scenarios**

1. **Given** the `Backend` trait, **When** a new backend is added, **Then** only a new module and
   one `cfg(target_os)` alias are needed; `run`'s orchestration is untouched.

## Requirements *(mandatory)*

### Functional

- **FR-1501**: THE SYSTEM SHALL define a `Backend` trait with: `resolve(command) -> (program,
  args)` (default = shell routing per SPEC-0013), `command(program, args, config, proxy) ->
  Result<Command>` (build the sandboxed process), and `exec_failure(program, status) ->
  Option<SandmeError>` (backend-specific exec-failure decode).
- **FR-1502**: THE SYSTEM SHALL select the backend by `cfg(target_os)` at compile time — `seatbelt`
  on macOS — with no runtime dispatch and no `dyn`.
- **FR-1503**: `sandbox::run` SHALL take `(config, proxy, command)` and own the cross-platform
  orchestration (backend `resolve`, backend `command`, env wiring, spawn, Ctrl-C, exit-status),
  delegating only backend-specific steps to the trait.
- **FR-1504**: THE macOS `seatbelt` backend SHALL produce byte-for-byte the same `sandbox-exec`
  invocation, SBPL profile, app-bundle redirect and `71`-decode as before this change.

### Non-functional

- **NFR-1501**: The refactor SHALL change no observable behaviour on macOS: the full existing test
  suite passes unchanged, with no test's assertions weakened.
- **NFR-1502**: The trait SHALL require no new dependency (no `async-trait`): `command` returns a
  `tokio::process::Command` for the shared async `run` to drive, so the trait stays synchronous.

## Interface contract

No CLI, configuration, or exit-code surface changes. Internal module layout:

```
src/sandbox/
  mod.rs               # Backend trait, run() orchestration, cfg(target_os) selection, shell_route
  seatbelt/
    mod.rs             # impl Backend for Seatbelt (macOS)
    profile.rs         # SBPL generation (was src/profile.rs)
    app_bundle.rs      # .app CLI-wrapper redirect (was src/app_bundle.rs)
```

`executable`, `tunnel`, `proxy`, `egress`, `credential`, `config`, `error` stay top-level
(cross-platform). `executable::locate`/`exec_failure` stay shared (PATH resolution is
platform-agnostic; the `71` trigger that calls it is the seatbelt backend's).

## Success criteria *(mandatory)*

- **SC-1501**: `cargo test` passes on macOS with the same test count and no weakened assertions.
- **SC-1502**: The generated SBPL and the `sandbox-exec` argv are unchanged (asserted by the
  migrated `profile` unit tests and the existing integration tests).
- **SC-1503**: Adding a backend needs only a new module + one `cfg` alias (demonstrated by the
  trait shape; the Linux backend is out of scope here).

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-1501, FR-1502, FR-1503 | the code compiles with the trait + `cfg` alias; `sandbox::run` signature is `(config, proxy, command)` |
| FR-1504, NFR-1501, SC-1501, SC-1502 | the migrated `profile::tests` and the full `tests/cli.rs` suite pass unchanged |
| NFR-1502 | `Cargo.toml` gains no dependency; the trait is synchronous |

## Assumptions

- The `.app` redirect and SBPL are only ever used on macOS, so moving them under `seatbelt` loses
  nothing. Verified: no non-macOS code references them.

## Open questions

- None. The Linux backend's design (Landlock ruleset mapping, kernel-version fallback, CI) is
  deferred to #8's follow-up spec.

## Implementation tasks

- [ ] **T-1501** — Add `src/sandbox/mod.rs` with the `Backend` trait, `shell_route`, `run`
      orchestration and the `cfg(target_os)` selection (covers FR-1501, FR-1502, FR-1503).
- [ ] **T-1502** — Move `profile.rs` and `app_bundle.rs` under `src/sandbox/seatbelt/` and add
      `seatbelt/mod.rs` implementing `Backend` (covers FR-1504).
- [ ] **T-1503** — Change `main::run` to `sandbox::run(&config, &server, &command)`; drop the
      top-level `profile`/`app_bundle` modules (covers FR-1503).
- [ ] **T-1504** — Run the full gate; confirm the suite passes unchanged (covers NFR-1501,
      SC-1501).

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-13 | Initial draft, with the implementation (backend abstraction; macOS behaviour unchanged). |
