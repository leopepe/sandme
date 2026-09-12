# SPEC-0014: Shipped behaviour no requirement covered

## Metadata

- **Status**: Implemented
- **Created**: 2026-09-12
- **Updated**: 2026-09-12
- **Related ADRs**: none yet
- **Related specs**: SPEC-0001 (extends; qualifies its Interface contract and SC-002)

## Summary

Several behaviours shipped into `main` without a requirement behind them: the profile's
unconditional `/dev/fd` grant, the app-bundle redirect that launches a different executable than
the user named, and the proxy's ephemeral-port binding when `proxy_port = 0`. This spec states
each as a requirement traceable to the tests that already prove it, and records the one remaining
gap — the `HOME`-unset config fallback — that has no test to cite. It is a reconciliation spec:
it documents what the code already does ([#34](https://github.com/leopepe/sandme/issues/34) item 5),
it does not change behaviour.

## Spec deltas

### ADDED

- **FR-1401** … **FR-1404** below.

### MODIFIED

- **SPEC-0001/Interface contract — the `<command> [args...]` operand row** — was: *"the child
  receives its own argument vector."* Now: on the multi-operand (direct-exec) path, WHERE the
  named program is a macOS app-bundle CLI wrapper, sandme launches the bundle's own main
  executable in place of the named program (FR-1402). The single-operand `/bin/sh -c` path is
  unaffected. Reason: SPEC-0001 stated the child runs exactly what the user named, but the
  app-bundle redirect (PR [#21](https://github.com/leopepe/sandme/pull/21)) deliberately runs a
  different program so that a sandboxed IDE wrapper — which would otherwise ask LaunchServices to
  open the app, a request the sandbox denies — starts at all.
- **SPEC-0001/SC-002** — was: *"A process running under sandme … cannot read or write filesystem
  locations outside the shared paths."* Now qualified by FR-1401: the profile additionally grants
  read+write on the `/dev/fd` subtree. `/dev/fd/N` names the process's *own* already-open file
  descriptors, not arbitrary filesystem locations, so the grant does not widen the set of files a
  sandboxed command can reach; SC-002 holds in substance. Recorded because the grant is, literally,
  a path outside `shared_paths` and a reader checking SC-002 against the profile needs it in view.

## Problem

`docs/guidelines/sdd/spec-driven-development.md` §1: when code and spec disagree, one of them is a
bug — and it must be named. Four shipped behaviours had no requirement, so none could be reviewed
against a contract, and two of them read as contradictions of SPEC-0001:

- **`/dev/fd`** (PR [#17](https://github.com/leopepe/sandme/pull/17)): the base profile emits
  `(allow file-read* file-write* (subpath "/dev/fd"))` unconditionally, a read+write grant outside
  `shared_paths` that SC-002 appears to forbid.
- **The app-bundle redirect** (`src/app_bundle.rs`, PR [#21](https://github.com/leopepe/sandme/pull/21)):
  193 lines and its tests run a *different program than the user named*, the opposite of SPEC-0001's
  "the child receives its own argument vector," and its stderr diagnostic appears in no error table.
- **`proxy_port = 0`**: accepted and meaningful (bind an OS-chosen ephemeral port), but unstated.
- **The `HOME`-unset config fallback**: `config_path()` returns `./config.toml` when `HOME` is
  unset, an interface detail no requirement or test covers.

## Goals

- State the `/dev/fd`, app-bundle-redirect and `proxy_port = 0` behaviours as requirements, each
  traceable to the test that already proves it.
- Reconcile SPEC-0001's Interface contract and SC-002 with what ships, via the deltas above.
- Record the `HOME`-unset fallback as an explicit tracked gap rather than leave it silent.

## Non-goals

- Changing any of these behaviours. This spec is documentation of shipped code, not a redesign.
- The IPv6 loopback listener: already specified — SPEC-0003 enforces its egress policy (with
  `[::1]` integration tests) and SPEC-0011/FR-1105 requires a diagnostic when its bind fails. No
  new requirement is needed here.
- Adding a test for the `HOME`-unset fallback (recorded as a gap under Assumptions).
- Narrowing the `/dev/fd` grant, or making the app-bundle redirect configurable.

## User scenarios *(mandatory)*

### Story 1 — Process substitution works inside the sandbox (P2)

As someone running a shell pipeline under sandme, I want `/dev/fd/N` paths to resolve, so that
process substitution (`diff <(a) <(b)`) works.

**Acceptance scenarios**

1. **Given** a sandboxed command, **When** it opens a `/dev/fd/N` path for one of its own open
   descriptors, **Then** the read or write succeeds.

### Story 2 — A GUI editor wrapper actually starts (P1)

As someone launching an IDE through its `/usr/local/bin`-style CLI wrapper, I want sandme to start
the app, so that the sandbox does not turn every editor into a LaunchServices denial.

**Acceptance scenarios**

1. **Given** a named program that is a macOS app-bundle CLI wrapper, **When** sandme launches it on
   the multi-operand path, **Then** it runs the executable named by the bundle's
   `Contents/Info.plist` `CFBundleExecutable`, not the wrapper.
2. **Given** a wrapper whose bundle `Info.plist` cannot be read or names no usable executable,
   **When** sandme launches it, **Then** sandme writes one `sandme: `-prefixed diagnostic to stderr
   and runs the program as written.

### Story 3 — The proxy takes any free port (P2)

As someone running sandme instances in parallel, I want `proxy_port = 0` to let the OS pick a free
port, so that concurrent runs do not collide on a fixed port.

**Acceptance scenarios**

1. **Given** `proxy_port = 0`, **When** sandme starts, **Then** the proxy binds an OS-chosen
   ephemeral port and the sandboxed command reaches it without any manual configuration.

### Edge cases

- **The named program is already the bundle's own main executable** → no redirect; run as written.
- **The `CFBundleExecutable` name contains `/`** → rejected as a path-escape; run as written.
- **`HOME` is unset** → `config_path()` returns the relative `./config.toml` (tracked gap; see
  Assumptions).

## Requirements *(mandatory)*

### Functional

- **FR-1401**: THE SYSTEM SHALL grant the sandboxed command read and write access to the `/dev/fd`
  subtree, so that `/dev/fd/N` paths a shell hands to process substitution resolve. The grant
  exposes the process's own open file descriptors only; it does not name any other filesystem
  location.
- **FR-1402**: WHERE a program named on the multi-operand (direct-exec) path resolves to an
  executable inside a macOS `.app` bundle whose `Contents/Info.plist` names a usable
  `CFBundleExecutable` that exists and differs from the resolved program, THE SYSTEM SHALL launch
  that bundle main executable in place of the named program. Otherwise it SHALL launch the program
  as written.
- **FR-1403**: IF a named program is an app-bundle CLI wrapper but its `Contents/Info.plist` cannot
  be read, names no usable `CFBundleExecutable`, or names one that does not exist, THEN THE SYSTEM
  SHALL write one `sandme: `-prefixed diagnostic to stderr and launch the program as written.
- **FR-1404**: WHEN `proxy_port` is `0` THE SYSTEM SHALL bind an operating-system-chosen ephemeral
  loopback port, discover the port actually bound, and publish that port to the sandboxed command
  in the proxy URL and in the Seatbelt profile's network-outbound rule.

## Interface contract

**Configuration**

| Key | Env var | Type | Default | Description |
| --- | --- | --- | --- | --- |
| `proxy_port` | `SANDME_PROXY_PORT` | integer (u16) | `8787` | Loopback port the egress proxy listens on. `0` means the OS picks a free ephemeral port (FR-1404). |

**Errors**

| Channel | Condition | Message to user |
| --- | --- | --- |
| stderr | FR-1403 — an app-bundle wrapper's `Info.plist` is unreadable or unusable | `sandme: <program> is the CLI wrapper of the app bundle <bundle>, but <reason>; running it unchanged, which macOS may refuse inside the sandbox` |

No new CLI flag and no new exit code.

## Key entities *(optional)*

- **App-bundle CLI wrapper**: a program on `PATH` that lives inside a `.app` bundle's
  `Contents/MacOS` directory (or resolves into one); its bundle's `Info.plist` `CFBundleExecutable`
  names the executable sandme runs instead.

## Constraints and dependencies

- macOS Seatbelt (`sandbox-exec -p`), per SPEC-0001 NFR-001. The `/dev/fd` grant and the network
  rule are lines in the generated profile.
- The app-bundle redirect applies only to the multi-operand direct-exec path; the single-operand
  `/bin/sh -c` path (SPEC-0001) is unaffected.
- The ephemeral-port discovery reads the port back from the bound listener; it is the single source
  of truth published to both the child and the profile.

## Success criteria *(mandatory)*

- **SC-1401**: A sandboxed process-substitution pipeline that reads `/dev/fd/N` succeeds.
- **SC-1402**: A sandboxed IDE launched through its app-bundle CLI wrapper starts the bundle's main
  executable, and an unreadable bundle produces the stderr diagnostic and still attempts the run.
- **SC-1403**: With `proxy_port = 0`, a sandboxed command reaches the proxy on the OS-chosen port
  with no manual configuration.

## Verification

| Requirement | Verified by |
| --- | --- |
| FR-1401 | `grants_dev_fd_read_write` (`src/profile.rs`) asserts the profile contains `(allow file-read* file-write* (subpath "/dev/fd"))` |
| FR-1402 | `finds_the_bundle_a_wrapper_lives_in`, `reads_the_executable_name_from_a_plist`, `ignores_an_executable_name_that_escapes_the_bundle` (`src/app_bundle.rs`) — the redirect's inputs: locating the bundle, reading the executable name, and the path-escape guard. (Gap: no single test asserts the assembled `main_executable` redirect end to end; see Assumptions.) |
| FR-1403 | `reports_an_app_bundle_it_cannot_read` (`tests/cli.rs`) drives the binary and asserts the stderr diagnostic; `ignores_a_plist_without_the_key`, `ignores_an_empty_executable_name` (`src/app_bundle.rs`) cover the unusable-`Info.plist` branches |
| FR-1404 | `routes_http_egress_through_the_proxy`, `no_manual_proxy_configuration_needed` (`tests/cli.rs`) — the whole integration suite runs with `SANDME_PROXY_PORT=0` (the `sandme_at` helper), so the child reaching the proxy proves the OS-chosen port is bound, discovered and published |

## Assumptions

- **The `/dev/fd` grant is not an SC-002 widening.** `/dev/fd/N` names descriptors the process
  already holds, so the grant cannot reach a file the process could not already reach. Captured as
  FR-1401 and reflected in the SC-002 delta rather than treated as a defect.
- **The app-bundle redirect's happy path is verified by its components, not end to end.** The
  bundle-location, plist-parse and path-escape unit tests exist; a single test asserting
  `main_executable` returns the redirect path for a valid bundle does not. Recorded as a tracked
  gap; the components together cover the decision.
- **Tracked gap — the `HOME`-unset config fallback is unspecified and untested.** `config_path()`
  returns the relative `./config.toml` when `HOME` is unset (`src/config.rs`), but no requirement
  states it and no test exercises it (`reads_config_file_from_default_path` covers only the
  `HOME`-set path). Left as a gap rather than an FR because a full requirement is disproportionate
  for a single fallback line with no test to cite; a future change that either drops the fallback
  or tests it should promote this to an FR. Does not block the FRs above.

## Open questions

None block this spec. The one unspecified detail — the `HOME`-unset config fallback — is recorded
as a tracked gap under Assumptions.

## Implementation tasks

- [x] **T-1401** — Retroactively state the `/dev/fd` grant as FR-1401, traceable to
  `grants_dev_fd_read_write` (behaviour shipped in PR #17).
- [x] **T-1402** — State the app-bundle redirect (FR-1402) and its diagnostic (FR-1403), and the
  MODIFIED SPEC-0001 Interface-contract delta (behaviour shipped in PR #21).
- [x] **T-1403** — State `proxy_port = 0` as FR-1404, traceable to the port-0 integration suite.
- [x] **T-1404** — Record the `HOME`-unset fallback as a tracked gap under Open questions.

## Changelog

| Date | Change |
| --- | --- |
| 2026-09-12 | Initial spec, reconciling shipped-but-unspecified behaviour from [#34](https://github.com/leopepe/sandme/issues/34) item 5: `/dev/fd` (FR-1401), the app-bundle redirect (FR-1402/FR-1403, MODIFYING SPEC-0001's argument-vector clause and SC-002), and `proxy_port = 0` (FR-1404). The `HOME`-unset config fallback is recorded as a tracked gap; the IPv6 listener is noted as already specified by SPEC-0003 and SPEC-0011. Status `Implemented`: every stated requirement's behaviour is on `main` and its Verification test exists. |
