# SPEC-0005: Process-information isolation

## Metadata

- **Status**: Implemented
- **Created**: 2026-09-10
- **Updated**: 2026-09-12
- **Related ADRs**: none yet
- **Related specs**: SPEC-0001 (modifies its baseline operation grants), SPEC-0006 (defers the
  sysctl allowlist to it), SPEC-0002 (shares its denial-ordering discipline)

## Summary

The Seatbelt profile granted `process-info-pidinfo` without restriction. A sandboxed command
could therefore read `KERN_PROCARGS2` for any process running under the same uid, which returns
that process's full argument vector and environment. This spec denies that read, keeps the
process information a command needs about its own sandbox, and leaves the rest of the
`sysctl-read` tree untouched.

## Spec deltas

### MODIFIED

- **SPEC-0001/FR-003**, the profile's baseline operation grants.
  Was: `(allow process-info-pidinfo)` — process information for any pid.
  Now: `(allow process-info-pidinfo (target same-sandbox))` — the command's own sandbox
  instance only (FR-302). Reason: the unscoped grant is what answered another process's
  environment.

### ADDED

- **FR-301** … **FR-305**, **NFR-301**, **NFR-302** below.

## Problem

Reported in [#31](https://github.com/leopepe/sandme/issues/31) and reproduced on macOS 26
(Darwin 25.6.0, Apple silicon). A probe of about thirty lines of C, compiled inside the sandbox,
reads another process's environment:

```text
$ sandme "$S/envsteal $HOLDER"
LEAKED: HOSTPROC_API_KEY=sk-supersecret-abc123
```

`ps -E` is not the route — it is setgid and the profile denies it, which made the profile look
tighter than it was. The route is `sysctl({CTL_KERN, KERN_PROCARGS2, pid})`, and it needs no
privileged setup.

This defeats sandme's stated purpose. The command being sandboxed is a coding agent the user does
not fully trust; it could read every environment variable of every process that user runs — API
keys, tokens, credentials handed to other tools — and send them out through the proxy's permitted
public egress, which is working as designed. Denying direct network access does not help: the
leak leaves by the sanctioned route. It also exposes the proxy credential SPEC-0003 publishes
into the child's environment as `HTTP_PROXY`.

`KERN_PROCARGS2` takes the pid in a numeric MIB, so no name-based `sysctl-read` rule matches the
read. Restricting the process-information grant is what closes it; a `sysctl` denial beside it is
not sufficient, and not necessary for the leak itself.

## Goals

- Deny a sandboxed command the environment of any process outside its own sandbox.
- Keep the process information a command needs to supervise what it starts.
- Keep every `sysctl` read that ordinary programs make.
- Make the rules hard to remove accidentally: each looks redundant beside the others.

## Non-goals

- **Narrowing `sysctl-read` to an allowlist.** `kern.proc.all` — the process table — stays
  readable, as do the other 1800-odd values `sysctl -a` reports. Deferred to SPEC-0006, which
  measured the change but carries a maintenance cost this spec does not take on: any tool needing
  an unlisted name gets `EPERM`.
- **Escaping `shared_paths` before interpolation.** `append_writable_grants` writes configuration
  into the profile unescaped, so a crafted value injects arbitrary SBPL. FR-304 makes the
  simplest payload fail; it does not close the class. Tracked as
  [#41](https://github.com/leopepe/sandme/issues/41).
- Hiding the sandboxed process's own environment from itself. It is the child's own environment,
  and `HTTP_PROXY` has to be in it for SPEC-0003/FR-203.
- The other unrestricted operations in the template — `mach-lookup`, `mach-register`,
  `mach-bootstrap`, `lsopen`, `iokit-open`. Raised by issue #12 part 4; a separate audit.

## User scenarios *(mandatory)*

### Story 1 — A sandboxed agent cannot read the credentials of other tools (P1)

As someone running a coding agent I do not fully trust, I want the sandbox to hide the
environments of my other processes, so that the API keys I gave those processes stay out of
the agent's reach.

**Acceptance scenarios**

1. **Given** a process outside the sandbox holding a secret in its environment, **When** a
   sandboxed command asks the kernel for that process's arguments and environment, **Then** the
   kernel refuses and the secret does not appear in the command's output.
2. **Given** the same process, **When** a `shared_paths` value injects an unscoped
   `(allow process-info-pidinfo)` and a blanket `(allow sysctl-read)` into the profile, **Then**
   the kernel still refuses.
3. **Given** a sandboxed command that forks, **When** it asks the kernel for its own child's
   process information, **Then** it gets an answer.
4. **Given** a sandboxed command, **When** it reads `hw.ncpu`, **Then** it gets a value.

### Edge cases

- **`$HOME` is unset or unresolvable.** The profile's home-relative denials cannot be built, and
  generation returns early. The rules here do not depend on `$HOME` and must survive that
  return — NFR-301.
- **A rule elsewhere in the profile grants the operation.** SBPL prefers a specific allow to a
  wildcard deny whatever the rule order, so a single wildcard denial is not enough — FR-304,
  NFR-302.
- **Two concurrent `sandme` runs.** Each is its own sandbox instance, so `same-sandbox` denies
  either one the other's environment, and with it the other's proxy credential.

## Requirements *(mandatory)*

### Functional

- **FR-301**: THE SYSTEM SHALL deny the sandboxed command `sysctl-read` of every name under the
  `kern.procargs` prefix.
- **FR-302**: THE SYSTEM SHALL grant `process-info-pidinfo` only where the target is inside the
  command's own sandbox instance.
- **FR-303**: THE SYSTEM SHALL deny the `process-info` operation family by an explicit rule, not
  by relying on `(deny default)`.
- **FR-304**: THE SYSTEM SHALL deny `process-info-pidinfo` by name in addition to the family
  wildcard of FR-303.
- **FR-305**: WHILE FR-301 holds THE SYSTEM SHALL still permit `sysctl-read` of names outside the
  `kern.procargs` prefix.

### Non-functional

- **NFR-301**: IF `$HOME` cannot be resolved THEN THE SYSTEM SHALL still emit the rules
  satisfying FR-301, FR-303 and FR-304.
- **NFR-302**: The generated profile SHALL contain no unscoped `process-info-…` allow. SBPL
  resolves a specific allow in preference to a wildcard deny regardless of rule order, so one
  such grant anywhere in the profile defeats FR-303.

## Interface contract

No change to the CLI surface, the configuration keys, the exit-status reservations (SPEC-0004) or
the child's environment. The change is confined to the generated SBPL.

## Constraints and dependencies

- macOS Seatbelt (`sandbox-exec`) is the enforcement mechanism. Its preference for a specific
  allow over a wildcard deny is the constraint FR-304 and NFR-302 exist to encode; SPEC-0002's
  last-match-wins ordering holds only between rules of equal specificity.
- `curl` requires process information about itself. Dropping the grant rather than scoping it
  fails four of SPEC-0003's integration tests.
- `(target same-sandbox)` rather than `(target self)`: measured, `self` refuses a command its own
  child's information, which would narrow a behaviour no requirement asks to narrow.

## Success criteria *(mandatory)*

- **SC-301**: A probe running inside the sandbox receives `Operation not permitted` when it reads
  `KERN_PROCARGS2` for a process outside the sandbox, and no marker planted in that process's
  environment appears in its output.
- **SC-302**: SC-301 continues to hold when a `shared_paths` value injects an unscoped
  `(allow process-info-pidinfo)` and a blanket `(allow sysctl-read)`.
- **SC-303**: A sandboxed command reads its own child's process information.
- **SC-304**: `hw.ncpu` resolves inside the sandbox, and `curl` through the proxy still reaches a
  public origin.
- **SC-305**: The quality gate passes — `cargo fmt`, `cargo clippy --all-targets`, `cargo build`,
  `cargo test`, `cargo doc --no-deps`.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-301, FR-303, FR-304 | `cli::denies_reading_another_process_environment` — red against the profile as it was; `profile::tests::denies_process_information_after_every_grant` asserts all three rules are present and follow the last allow |
| FR-302 (denies outside) | `cli::denies_reading_another_process_environment` |
| FR-302 (allows inside) | `cli::reads_its_own_child_process_information` |
| FR-305 | `cli::reads_a_sysctl_outside_the_denied_prefix`; `profile::tests::denies_process_information_after_every_grant` asserts the grant survives |
| NFR-301 | `profile::tests::denies_process_information_without_a_home` |
| NFR-302 | `profile::tests::denies_process_information_after_every_grant` |
| SC-301, SC-303, SC-304 | the integration tests above |
| SC-302 | `cli::denies_reading_another_process_environment_under_an_injected_grant` |
| SC-305 | the gate, run in the order `docs/guidelines/code/quality-gates.md` §2 gives |

Measured on macOS 26 (Darwin 25.6.0, Apple silicon), against the built binary:

| Read | before | after |
| --- | --- | --- |
| another process's environment (`KERN_PROCARGS2`) | readable, marker visible | **denied** |
| the same, under an injected unscoped grant | readable | **denied** |
| the command's own child's process information | readable | readable |
| `hw.ncpu`, `kern.osrelease` | readable | readable |
| `sysctl -a` values | 1844 | 1844 — see SPEC-0006 |

Each of FR-301, FR-303 and FR-304 was dropped on its own and the read succeeded, which is why all
three are stated.

## Assumptions

- `kern.procargs` is the only `sysctl` name prefix returning another process's environment. Other
  `kern.proc*` names were not exhaustively enumerated; `kern.proc.all` is readable and returns
  the process table, which SPEC-0006 addresses.
- `(target same-sandbox)` scopes to one `sandme` invocation, so two concurrent runs cannot read
  each other. Verified for the environment read; not enumerated for every `process-info`
  sub-operation.

## Open questions

Resolved before acceptance: FR-303's `(deny process-info* ...)` wildcard covers every
`process-info` sub-operation, `pidinfo` included, so no sub-operation is left reachable. Whether a
future macOS adds a process-info route worth a narrower rule is a follow-up for `/security-audit`,
not a blocker on this spec — recorded as a non-goal here.

## Implementation tasks

- [x] **T-001** — Scope the template's process-information grant to the sandbox instance
      (covers FR-302).
- [x] **T-002** — State the three denials before the profile's home-relative early return
      (covers FR-301, FR-303, FR-304, NFR-301).
- [x] **T-003** — Unit tests for rule presence, ordering, and the absence of an unscoped grant
      (covers FR-305, NFR-301, NFR-302).
- [x] **T-004** — Integration tests driving a kernel probe inside the sandbox, each with its
      unsandboxed control (covers FR-301, FR-302, FR-305, SC-301 … SC-304).
- [x] **T-005** — `/security-audit`, required by `AGENTS.md`: `.agents/reports/security-audit-2026-09-10.md`. One High finding (F1), predating this change; the branch itself is clean.

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-10 | Initial draft, written alongside the implementation. |
| 2026-09-12 | Status `Draft` → `Accepted` → `Implemented`. The process-information isolation shipped with PR [#39](https://github.com/leopepe/sandme/pull/39); all tasks are ticked and every Verification test exists. Resolved the one open question (FR-303's family wildcard covers every `process-info` sub-operation; a narrower rule is a `/security-audit` follow-up, not a blocker) ([#34](https://github.com/leopepe/sandme/issues/34)). |
