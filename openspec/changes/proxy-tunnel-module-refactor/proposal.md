## Why

The proxy server's lifecycle and request processing currently share a flat source file, while the SSH CONNECT client has a separate flat file. SPEC-0018 (Accepted; FR-1801–FR-1803, NFR-1801–NFR-1802) requests independently discoverable, documented interfaces without changing the caller-visible contracts; accepted ADR-0001 fixes the layout.

## What Changes

- Carry out the four canonical SPEC-0018 tasks, T-1801 through T-1804, in order in the existing single-writer worktree. First repair the seven specified baseline lint findings and record baseline evidence; then characterize and relocate the proxy and SSH client; finally repoint active verification/review references and complete the required reviews and platform verification.
- Use the sibling `proxy/` and `tunnel/` components and declarative facades selected by ADR-0001 D1–D6. Preserve the interfaces, security decision chain, command dispatch, behavior and test assertions identified in SPEC-0018. Both macOS and Linux are affected; compatible Linux execution remains a completion prerequisite, not a claim about present coverage.
- No spec-level behavior delta is proposed. The accepted `docs/specs/0018-proxy-and-tunnel-module-layout.md` is the requirement authority; `.openspec.yaml` opts out of OpenSpec delta specs with `skip_specs: true`.

## Capabilities

### New Capabilities

None; this is a behavior-preserving source refactor under accepted SPEC-0018.

### Modified Capabilities

None; existing behavior requirements do not change. The accepted maintainer-facing organization requirements and verification are in SPEC-0018, not a parallel OpenSpec spec.

## Impact

Source, test and active reference paths are limited to the `touches` annotations in SPEC-0018 T-1801–T-1804. The owner-approved T-1804 scope includes pointer-only updates in `CONTRIBUTING.md` and `.github/pull_request_template.md` for `src/proxy/` and `src/tunnel/`, retaining other review coverage. Crate-facing paths and signatures remain those in SPEC-0018's Interface contract; Cargo dependencies and production targets remain unchanged (NFR-1801, NFR-1802). After a clean gate, security review must explicitly cover the moved `src/proxy/` and `src/tunnel/` trees as well as previously protected paths.

## Non-goals

No new proxy/SSH behavior, modes, configuration, diagnostics, status codes, dependency, production target, abstraction, sandbox or credential ownership change; no legacy-status reconciliation, unrelated cleanup, remote action or release. Additional lint classes, defects, paths or behavioral changes require a separate disposition, not an expansion of these tasks.
