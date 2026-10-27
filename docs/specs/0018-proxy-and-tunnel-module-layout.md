# SPEC-0018: Proxy and tunnel module organization

## Metadata

- **Status**: Accepted
- **Created**: 2026-10-22
- **Updated**: 2026-10-27
- **Related ADRs**: ADR-0001 (`docs/adrs/0001-proxy-and-tunnel-module-boundaries.md`)
- **Related specs**: SPEC-0001, SPEC-0003, SPEC-0011, SPEC-0012, SPEC-0014 (preserves); SPEC-0015 and SPEC-0017 (shipped-source context only; both currently Draft)

## Summary

Organize the proxy server and SSH CONNECT client as separately discoverable components,
with their published interfaces separated from implementation. This is a behavior-preserving
source refactor for contributors; the placement and visibility decisions are in ADR-0001.
This document is the requirements and build plan, not permission to implement before approval.

## Problem

The proxy's listener lifecycle and request processing occupy one 442-line file, while its
exclusive egress policy sits beside unrelated crate-level components. The SSH helper also
sits at the root even though its purpose is distinct from the proxy server. Contributors
cannot inspect either component's interface independently of its executable implementation.

## Goals

- Make the two component boundaries independently discoverable through their documented interfaces.
- Keep the existing crate-facing interfaces and runtime contracts unchanged.
- Keep verification references and security-review coverage valid after source relocation.
- Supply bounded tasks and observable checks for a fresh-context Conductor implementation.

## Non-goals

- New proxy protocols, modes, configuration, flags, status codes or production diagnostics.
- Rewriting authentication, entropy fallback, DNS resolution, header handling or relay behavior.
- New traits, wrappers, policy/codec frameworks, dependencies, production targets or CI infrastructure.
- Moving the shared credential module or changing configuration/error ownership.
- Moving child environment wiring or modifying sandbox implementation, apart from the separately
  owner-approved warning-only prerequisite in T-1804; it may not change sandbox behavior.
- Converting existing sandbox `mod.rs` files to facades.
- Reconciling legacy Draft spec statuses or fixing unrelated findings discovered during review,
  apart from the explicitly owner-approved warning and test-diagnostic checkpoints in T-1804.
- Performance optimization or a new startup-latency budget.

## User scenarios *(mandatory)*

### Story 1 — Inspect each component without reading transport code (P1)

As a contributor, I want a documented entry interface for each responsibility so I can find
its implementation without treating the SSH client as part of the server.

**Acceptance scenarios**

1. **Given** a checkout implementing this change, **When** a contributor opens either component's
   entry file, **Then** its purpose, component declarations and explicit crate-facing exports
   are visible without executable implementation (FR-1801, FR-1802).
2. **Given** the existing callers in `main` and `sandbox`, **When** the crate is compiled,
   **Then** they keep using the existing interfaces without importing implementation children
   (FR-1802, NFR-1801).

### Story 2 — Use sandme without noticing the refactor (P1)

As a sandme user, I want existing proxied commands and git-over-SSH transport to behave as before.

**Acceptance scenarios**

3. **Given** an ephemeral proxy port, **When** the CLI publishes its actual proxy address,
   **Then** that port is nonzero and accepts IPv4 and IPv6 loopback connections (NFR-1801).
4. **Given** the existing proxy contract fixtures, **When** they exercise forwarding,
   authentication, restricted destinations and headers, **Then** their existing assertions
   still pass, including use of vetted socket addresses rather than a second lookup (NFR-1801).
5. **Given** the existing SSH transport fixtures, **When** they exercise command wiring,
   a user override, a successful byte relay, missing proxy configuration or a refused CONNECT,
   **Then** command values, byte flow, diagnostics and statuses remain unchanged (NFR-1801).
6. **Given** a proxy-disabled invocation or a child that finishes, **When** the existing CLI
   cases run, **Then** their no-proxy and proxy-lifetime assertions remain unchanged (NFR-1801).

### Story 3 — Follow the proof and review the moved code (P2)

As a reviewer, I want active references to resolve to the moved implementation without losing
any existing security-review coverage.

**Acceptance scenarios**

7. **Given** an active verification or review pointer affected by this move, **When** a reviewer
   follows it, **Then** it reaches the current location and retains the same requirement and
   review obligation (FR-1803).
8. **Given** the repository's Cargo manifests and targets before and after this change,
   **When** they are compared, **Then** no dependency or production target has been added (NFR-1802).

### Edge cases

- Ephemeral ports, IPv6 bind warnings, invalid credentials, restricted IPv4/IPv6 destinations,
  private-egress opt-in, origin-form/Host rewriting and hop-by-hop headers retain existing
  behavior; security review inspects DNS pinning as well as running fixtures (NFR-1801).
- SSH helper dispatch remains before normal CLI/configuration processing and starts no second
  proxy or sandbox; credentials stay out of the command string (NFR-1801).
- Existing proxy-disabled behavior and user-supplied `GIT_SSH_COMMAND` remain unchanged (NFR-1801).
- Existing lint failures are a bounded prerequisite, not permission for general cleanup (T-1801).
- Linux execution unavailable on the implementation host is an explicit verification gap,
  not a passing Linux result (NFR-1801).

## Requirements *(mandatory)*

### Functional

- **FR-1801**: THE SYSTEM SHALL present the proxy-server responsibility and SSH CONNECT-client
  responsibility as separately documented components, following ADR-0001.
- **FR-1802**: THE SYSTEM SHALL expose each component's crate-facing interface independently
  of its executable implementation, without requiring callers to access implementation children.
- **FR-1803**: WHEN source relocation changes an active verification or review pointer THE
  PROJECT SHALL update that pointer while preserving its requirement identity and review coverage.

### Non-functional

- **NFR-1801**: The change SHALL introduce 0 changes to the existing caller-visible interfaces
  or runtime contracts listed in Interface contract, and SHALL remove or weaken 0 existing
  behavioral assertions. Test module paths may change; test leaf names and verified behaviors remain.
- **NFR-1802**: The change SHALL add 0 Cargo dependencies and 0 production binary or library targets.
  An integration-test target is not a production target.

## Interface contract

**Crate-facing interfaces** — the paths and signatures callers retain:

| Path | Contract |
| --- | --- |
| `proxy::serve` | `fn serve(config: &Config) -> Result<Server, SandmeError>` |
| `proxy::Server::addr` | `fn addr(&self) -> SocketAddr` |
| `proxy::Server::url` | `fn url(&self) -> String` |
| `proxy::Server` | Dropping the handle retains its existing accept-loop cleanup behavior |
| `tunnel::PROXY_COMMAND_ARG` | `&str` value `--sandme-ssh-connect` |
| `tunnel::git_ssh_command` | `fn git_ssh_command(sandme_exe: &Path) -> String` |
| `tunnel::run` | `async fn run(host: &str, port: &str) -> Result<(), SandmeError>` |

**Runtime surface** — no additions, removals or changed defaults. Preserve SPEC-0001's
launch/streams/lifetime contract, SPEC-0003's credential and egress contract as qualified by
SPEC-0011, SPEC-0012's SSH transport contract, and SPEC-0014's actual bound-port publication.
Preserve the current optional-proxy behavior implemented in source and described by Draft
SPEC-0017; this refactor does not approve or change that older spec's Status. Error ownership
and status mapping remain in the existing `error` and `main` modules.

This spec adds no product-behavior delta and supersedes no existing requirement. It adds
maintainer-facing organization constraints; ADR-0001 owns the actual structure.

## Constraints and dependencies

- Use the existing Rust 2024 binary, its declared Rust 1.88 floor and its current dependencies.
- ADR-0001 supplies the destination files, visibility, shared-code ownership and rejected alternatives.
- Repository guidelines remain authoritative; read the applicable directories named by `AGENTS.md`.
  In particular, use `docs/guidelines/code/quality-gates.md`, `docs/guidelines/tests/` and
  `docs/guidelines/performance/`; do not weaken their rules to make this refactor pass.
- The repository spec workflow governs approval. This spec stays Draft and ADR-0001 stays
  Proposed until the owner explicitly approves the written package. Open questions must be empty
  before those statuses become Accepted. No production code is authorized by preparation alone.
- `docs/specs/` remains the requirement authority. Conductor's existing OpenSpec working area
  may reference these IDs under `openspec/config.yaml`; it must not create a second requirement
  baseline. Preserve the existing schema/store rather than installing or initializing tooling.

## Success criteria *(mandatory)*

- **SC-1801**: Both component interfaces satisfy ADR-0001 and existing callers compile without
  implementation-child imports (FR-1801, FR-1802).
- **SC-1802**: Every existing behavioral test still exists and passes; the relocated live-listener
  case retains both address families and the actual ephemeral-port checks (NFR-1801).
- **SC-1803**: Active source pointers resolve, required reviews cover the moved code, and the
  Cargo dependency/target surface is unchanged (FR-1803, NFR-1802).

## Verification

| Requirement | Scenario | Verified by |
| --- | --- | --- |
| FR-1801 | 1 | ADR-0001 D1/D4 source-layout inspection; component rustdoc; T-1802 and T-1803 |
| FR-1802 | 1, 2 | ADR-0001 D2/D3 inspection of both facades and item visibility; `cargo build`; existing caller diff; T-1802 and T-1803 |
| FR-1803 | 7 | Active-reference audit and requirement/test-location map in T-1804; standards, architecture and spec-review reports |
| NFR-1801 | 2–6 | Full baseline/post-change inventory and CLI suite; named checks and expected results in T-1801–T-1804; security and performance reports; Linux execution evidence or explicit unresolved gap |
| NFR-1802 | 8 | Compare `Cargo.toml` and `Cargo.lock` to the source base; confirm the same single binary and no library; T-1804 |

### Regression and review focus

These five conditions are the review priorities; tests alone do not prove their full security
or lifecycle semantics. Tasks below retain their existing checks and add only the named missing
SSH failure characterizations.

| Condition | Tests / inspection |
| --- | --- |
| Actual ephemeral port and IPv6 warning | `serves_both_loopback_families_on_the_published_port` (move to CLI), `warns_when_the_ipv6_loopback_listener_cannot_bind` |
| Auth before resolution; pinned addresses and sanitized forwarding | Existing `refuses_a_local_client*`, `refuses_to_relay*`, `relays_to_host_loopback_when_private_egress_is_allowed`, `strips_hop_by_hop_headers_and_pins_host_to_the_vetted_authority`; inspect the single-resolution path |
| SSH success and refusal without normal startup | Existing `reaches_an_origin_through_the_ssh_connect_tunnel`; planned `reports_an_ssh_tunnel_without_a_usable_proxy` and `reports_a_refused_ssh_connect_with_the_reserved_status` |
| Proxy-off and user's SSH override | Existing `proxy_disabled_wires_no_proxy_and_denies_egress`, `keeps_a_user_supplied_git_ssh_command`, `wires_the_git_ssh_command_into_the_child` |
| Lifetime, byte flow and secret exposure | Existing `proxy_lifetime_follows_command`, relay test and command-builder test; inspect unchanged Drop/spawn/upgrade/stdio ordering and command values |

## Assumptions

- Keeping `credential.rs` shared at the root is the narrow implementation default from the
  discussed layout. A generalized authentication/codec component is not needed.
- `server.rs`, `request.rs` and `client.rs` are the purpose names selected for the written
  proposal. Acceptance of this package ratifies them; workers must not silently invent a new layer.
- The selected facade convention is limited to the two components, not a repo-wide migration.
- A new characterization may expose an existing defect. Record it and stop rather than
  changing the product contract under a refactor task.

## Open questions

None about the proposed scope. Written spec/ADR approval, execution-model permissions and
availability of a Linux validation host remain execution prerequisites, not inferred approvals.

## Implementation tasks

This is the canonical build plan. Conductor imports these task IDs and annotations into its
working task list, referencing this spec and ADR-0001 instead of duplicating requirements.
Use the operator-selected Conductor execution method, not an unrelated implementation runner.

Each task has one independently verifiable deliverable. Common validation and status bookkeeping
are coordinated by one integration owner; workers must not independently edit this spec.
This plan uses one execution worktree: formatting and the full-crate gate read both components,
so T-1803 waits for T-1802's stable, verified checkpoint rather than observing a partial move.
Do not launch concurrent writers in that cwd. End each Rust-source task with the full gate in `docs/guidelines/code/quality-gates.md`, including docs,
and read the output before committing or requesting review. Code changes are not complete while
Clippy reports any of the known errors/warnings.

### T-1801 — Restore the bounded baseline and freeze evidence

- [x] 1. Repair only the already reproduced lint findings and record the pre-relocation contract (NFR-1801).
  depends-on: none
  touches: src/proxy.rs, src/config.rs

**Interfaces:** consumes the existing source at the handover's base; produces the same interfaces
and behavior, a warning-free baseline, and an exact commit/test inventory for subsequent tasks.

1. Reconfirm owner approval of this spec and ADR before editing source. Read the original
   baseline logs referenced by the handover and reproduce `cargo clippy --all-targets`.
2. At `src/proxy.rs:98–102`, remove the four unnecessary borrows of the `fail` closure in
   `map_err`. At `src/config.rs:439,445,474`, express the same empty/nonempty assertions with
   `assert_eq!`/`assert_ne!` against an explicitly typed empty `Vec<String>`. Change no conditions,
   effective values, production configuration logic, test names or lint configuration.
3. Run the complete quality gate and save command output, exit codes, toolchain and platform
   under the local Conductor evidence directory. Expected: no lint/build/doc warnings; all tests
   pass. At the planning base, macOS ran 78 unit + 62 integration tests successfully, with one
   intentionally ignored `probe` fixture (`tests/cli.rs:1488–1495`). Preserve that fixture's reason.
4. Save `cargo test --bin sandme -- --list` and `cargo test --test cli -- --list` inventories,
   and capture release-build startup evidence: three warmups, then at least 20 samples each of
   `./target/release/sandme -- /usr/bin/true` and bare `/usr/bin/true`. Use an isolated temporary
   HOME/cwd, clear inherited `SANDME_*` overrides except `SANDME_PROXY_PORT=0`, and require status
   0 for every sample. Retain raw timings, medians and wrapper-minus-bare overhead, exact baseline
   revision and environment for the post-change comparison. Follow performance-review's timing
   method; do not invent a latency budget. Commit only this prerequisite and approved planning
   records after the gate is clean, citing NFR-1801 and the §9 lint-only exception.

**Stop:** any additional lint class, test failure or proposed config behavior change needs a
separate disposition. Do not expand this task into a config refactor or suppress warnings.

### T-1802 — Extract the proxy's existing responsibilities

- [x] 2. Implement the proxy boundary and relocate its tests without changing the request path (FR-1801, FR-1802, NFR-1801, NFR-1802).
  depends-on: 1
  touches: src/main.rs, src/proxy.rs, src/egress.rs, src/proxy/mod.rs, src/proxy/server.rs, src/proxy/request.rs, src/proxy/egress.rs, tests/cli.rs

**Interfaces:** consumes the unchanged `Config`, `Credential` and `SandmeError`; produces the
unchanged `proxy::serve`, `proxy::Server::addr` and `proxy::Server::url` interfaces. Internal
`route(req: Request<Incoming>, policy: Policy) -> Result<Response<Body>, Infallible>` and
`Policy` are shared only within `proxy/` as decided by ADR-0001 D3.

1. Record the expected pre-change structural failures: `src/proxy/mod.rs` does not exist and
   the old flat files do. Retain the current passing destination, range and CLI checks as the
   characterization net; no new runtime behavior is being requested.
2. Move the live-listener behavior into the CLI integration suite, retaining its leaf name.
   Drive the built binary with `SANDME_PROXY_PORT=0`; have the sandboxed child publish only the
   address part of its proxy URL, never the credential, into its temporary work directory.
   Assert the published port is nonzero and both loopback families accept that same port.
   Bound discovery and process cleanup, and close/reap the fixture on success and failure.
   Run this integration case against the still-flat implementation before removing the old
   unit copy. Preserve all existing CLI assertions and helpers unrelated to this case.
3. Move definitions to the ADR-0001 destinations; move range tests with egress and parsing
   tests with request processing. Remove the flat `proxy.rs` and `egress.rs` and only the root
   `mod egress;` declaration. Keep `main` dispatch/order and `sandbox` callers otherwise unchanged.
   Apply only the visibility/import adjustments needed by the split. Do not create a new resolver,
   policy API, wrapper, background task or error type.
4. Run `cargo test --bin sandme proxy:: -- --nocapture` and the named regression tests above,
   including the moved listener case with this single command:
   `cargo test --test cli serves_both_loopback_families_on_the_published_port -- --exact`.
   Expected: every selected test actually runs and passes. Then run the full gate, inspect
   the auth → vetted-resolution → forward/CONNECT chain against the baseline, and commit.

### T-1803 — Extract the SSH CONNECT client

- [x] 3. Implement the tunnel facade while keeping the command/runner contract together (FR-1801, FR-1802, NFR-1801, NFR-1802).
  depends-on: 2
  touches: src/tunnel.rs, src/tunnel/mod.rs, src/tunnel/client.rs, tests/tunnel.rs

**Interfaces:** consumes `crate::credential::base64` and `SandmeError`; produces the same
`tunnel::PROXY_COMMAND_ARG`, `tunnel::git_ssh_command` and `tunnel::run` signatures/values in
Interface contract. No changes to `main` or `sandbox` callers are needed.

1. Add `tests/tunnel.rs` characterizations before relocation. In
   `reports_an_ssh_tunnel_without_a_usable_proxy`, invoke the built binary directly with the
   sentinel and both proxy variables removed; assert status 125, empty stdout and the existing
   `no usable HTTP_PROXY` diagnostic. Supply an invalid normal `SANDME_PROXY_PORT` value to prove
   helper dispatch does not enter normal configuration/startup.
2. Add `reports_a_refused_ssh_connect_with_the_reserved_status`, covering 403 and 407 responses
   from a bounded local proxy fixture. Invoke the real binary helper with a fixture proxy URL;
   verify the fixture receives CONNECT for the exact target and the expected Basic header,
   then assert status 125, empty stdout and the existing diagnostic containing target and HTTP
   status. All socket/process operations and fixture joins need deadlines/cleanup; join the
   fixture result and propagate its assertion failures so a fixture crash cannot masquerade
   as the expected refusal. No real SSH server, internet lookup, keys or production credentials are used. Run both tests against
   the still-flat source; expected: pass before the move. A failure is a baseline issue, not
   permission to change tunnel behavior.
3. Move the existing module body and command-builder test into `client.rs`; create only the
   declarative facade described by ADR-0001 D2; remove flat `src/tunnel.rs`. Preserve helper
   dispatch, command quoting, environment reads, status parsing and stdio/task ordering.
4. Run the two characterization tests and the existing command-builder, SSH wiring, override
   and relay tests. Confirm filters run nonzero tests, then run the full gate and commit.

### T-1804 — Reconnect traceability and verify the whole change

- [x] 4. Update active pointers, perform the required reviews, and record complete verification (FR-1801, FR-1802, FR-1803, NFR-1801, NFR-1802).
  depends-on: 2, 3
  touches: AGENTS.md, src/AGENTS.md, openspec/config.yaml, .agents/skills/security-audit/SKILL.md, docs/specs/0003-proxy-egress-restrictions.md, docs/specs/0011-proxy-hardening-follow-ups.md, docs/specs/0012-ssh-connect-tunnel.md, docs/specs/0018-proxy-and-tunnel-module-layout.md, CONTRIBUTING.md, .github/pull_request_template.md, src/error.rs, src/executable.rs, src/sandbox/landlock/mod.rs, src/main.rs, tests/cli.rs

**Interfaces:** consumes the stable facades and all test evidence from tasks 1–3; produces
resolved active verification references, unchanged security-review coverage and an honest
completion/blocked report. The integration owner alone updates canonical task/status records.

**Owner-approved warning-only prerequisite (NFR-1801 / §9):** resolve only the
four Linux warning sites in `src/error.rs` (`UnsafeProfilePath`), `src/executable.rs`
(`locate`), and `src/sandbox/landlock/mod.rs` (documentation backticks and borrowed error
formatting). `src/main.rs::failure_code` is the required companion for the platform-gated
error variant; preserve status 125 and every other mapping. These four source files are the
warning-only exception to the refactor's source-preservation exclusions. Preserve error text, policy,
ABI/enforcement, startup/dispatch, interfaces and assertions; add no lint suppression. Run the
full gate and applicable independent reviews before the separately approved lint-only commit
and push to the existing CI draft. The integration task remains open until all obligations pass.

**Owner-approved diagnostic checkpoints (NFR-1801 / §9):** native macOS runs reported
`000` instead of `403` in the metadata and then benchmarking refusal cases. Add shared,
failure-only exit status, elapsed time and bounded, redacted stderr to the existing
metadata, CGNAT and benchmarking cases in `tests/cli.rs`, with one private test-only
formatting helper in that file. Preserve every `403` and `< 5 seconds` assertion,
current curl arguments, capture/cleanup sequence, leaf name and all other test bodies.
Keep formatting lazy on assertion failure and retain the metadata diagnostic bound and
redaction behavior. This is not a production fix or a waiver of either failed gate. Run the full local gate and
required scoped review before the separately approved test-only commit and one push to
the existing CI draft. Do not add retries, skip the case, accept `000`, alter proxy
environment/policy or claim the cause was fixed when a later run merely passes.

1. Repoint source-location references in the older specs' **Verification** tables: egress tests
   to `src/proxy/egress.rs`; destination parsing to `src/proxy/request.rs`; command builder and
   runner to `src/tunnel/client.rs`; server-side request handlers to `src/proxy/request.rs`.
   In SPEC-0012, replace the manual FR-1205 proof with the new refusal characterization as well
   as its existing source mapping. Add location/test-proof Changelog notes only. Do not change
   older requirements or statuses. Leave historical narrative/archives/reports untouched.
2. Update existing active module/review pointers in the declared guidance files. Security
   triggers must cover `src/proxy/` and `src/tunnel/` and retain every other protected source
   path. In `CONTRIBUTING.md` and `.github/pull_request_template.md`, limit edits to these
   proxy/tunnel review pointers; preserve all other review coverage. Do not add a separate
   maintained source catalog or duplicate a guideline.
3. Verify both facades against ADR-0001 D2, confirm the three obsolete flat files are gone,
   audit each active pointer and test name, and compare test inventories by leaf name/behavior.
   The moved listener test changes unit/integration location, not coverage. Extra tests from
   task 3 are expected. Confirm no edits to `src/credential.rs`, `Cargo.toml` or `Cargo.lock`;
   error, executable, sandbox and main changes are limited to the warning-only exception above;
   test changes after T-1803 are limited to the shared three-case diagnostic exception above.
   Run the full gate on the integrated snapshot, including docs.
4. With a clean gate, run `/review-standards`, `/security-audit`, `architecture-review`,
   `spec-review` and `performance-review` as required by the repository's review table.
   Supply the new paths explicitly so a stale filename cannot exclude moved code. Performance
   review must compare the recorded baseline with at least 20 equivalent post-change release
   samples; no speed improvement or budget compliance may be claimed without measurements.
   Reviewers report only; any fix is a separate bounded assignment followed by the full gate.
5. Run build/tests on a compatible Linux host (kernel 6.7+, Landlock ABI v4), retaining exact
   command/toolchain/kernel evidence. macOS success, cross-compilation or platform-neutral plan
   tests do not prove Linux kernel enforcement. If unavailable, retain the gap and request
   maintainer disposition; do not report both platforms verified or silently mark this task done.
   Once the change-specific verification and required project reports are satisfied, the
   integration owner records their scoped disposition and ticks the Apply tasks. Keep this
   spec Accepted while Conductor's formal adversarial review, integrated Verify and Archive
   remain pending; project reports do not replace those cycle-level gates. Only after those
   review/verification gates may final Archive bookkeeping set Implemented and add its
   Changelog row, without overwriting the canonical requirement record. No PR/push/release
   is authorized here.

**Apply closure — 2026-10-27:** The owner accepted the refactor-scoped disposition and
local documentation checkpoint. G16's structural/reference/manifest evidence is recorded;
G17's native Linux execution is present. CI [40474050827](https://github.com/leopepe/sandme/actions/runs/40474050827)
tested source commit `bec9454`: macOS 142 passes plus one documented fixture, Linux x86_64
and aarch64 95 passes each, and warning-free Cargo gates including fmt/docs. The job-linked
Linux image inventory is kernel-version evidence, not a newly executed uname or numeric ABI
probe. Required project reports and scoped follow-ups are consumed with their limits; the
raw repository-wide spec BLOCK and pre-existing C1–C10/G1–G15 debt remain open outside this
refactor's fixes, not waived or relabelled passed. Startup observations retain their ordering
and unmeasured-dimension qualifications; earlier CI failures retain their unknown causes.
All Apply tasks are complete, but Status remains Accepted pending formal Conductor Phases 5–7.

## Changelog

| Date | Change |
| --- | --- |
| 2026-10-22 | Initial Draft requirements and build plan following the owner's selected sibling-module and declarative-facade design; no implementation performed. |
| 2026-10-23 | Owner explicitly accepted SPEC-0018 and ADR-0001, including T-1801's bounded lint repair and T-1802/T-1803's test work, in the Conductor implementation session. Implementation and verification remain pending. |
| 2026-10-23 | Owner approved adding `CONTRIBUTING.md` and `.github/pull_request_template.md` to T-1804 for proxy/tunnel review-pointer updates only, preserving all other review coverage; no requirement or runtime change. |
| 2026-10-23 | T-1801: repaired only the seven specified lint findings and froze a warning-free macOS baseline, preserved test inventories, and release startup samples. Proxy/tunnel relocation, required reviews and Linux verification remain pending; Status stays Accepted. |
| 2026-10-24 | T-1802: extracted the proxy boundary with unchanged function bodies, preserved all test leaves, and verified the real-binary same-port IPv4/IPv6 407/Basic characterization before relocation. Worker and parent full gates passed; tunnel extraction, active-pointer updates, required reviews and Linux verification remain pending. Status stays Accepted. |
| 2026-10-24 | T-1803: moved the SSH CONNECT implementation byte-for-byte behind its declarative facade; both named real-binary failure tests passed before relocation, including 403 and 407 exchanges. Preserved all existing leaves and added the two planned cases; worker final and parent gates passed without warnings. Active-pointer updates, required reviews and Linux verification remain pending; Status stays Accepted. |
| 2026-10-26 | Recorded the owner's approval to fix the four reproduced Linux warning sites and update the existing CI draft. Recorded the warning-only T-1804 exception and its required main status-arm companion; no requirement, runtime contract, assertion or Status change. The three-file diagnostic count is not the complete four-file repair footprint. Implementation and warning-free CI remain pending. |
| 2026-10-26 | Owner approved failure-only metadata-test diagnostics and one existing-draft CI push after read-only macOS investigation. Preserve exact `403` and `< 5 seconds` assertions; no production, requirement, interface, Status or completion change. The original CI failure remains recorded; diagnostic implementation and new CI are pending. |
| 2026-10-27 | Owner approved shared failure diagnostics for metadata, CGNAT and benchmarking denial cases and one further existing-draft CI checkpoint. The builder selected one private test-only formatter. Preserve all predicates, arguments and runtime behavior; no requirement/interface/Status/completion change. Both original native macOS failures remain recorded. |
| 2026-10-27 | T-1804 Apply closure approved: active-pointer updates, preserved interfaces/assertions/dependency surface, required project reports and qualified structural/native-platform evidence are recorded. All four Apply tasks are complete; legacy spec debt and historical failures remain open/preserved. This is a local-only documentation checkpoint, not final review, Archive, merge or release; Status stays Accepted pending formal Conductor Phases 5–7. |
