# Security audit — 2026-09-09

Branch `fix/process-and-sysctl-visibility` (SPEC-0005, issue #31). macOS 26, Darwin 25.6.0,
Apple silicon. Gate at audit time: fmt ✓ · clippy ✓ 0 warnings · build ✓ · test ✓ 38 unit +
40 integration, 1 ignored (the SPEC-0005 probe, by design).

## Summary

The change under audit closes the environment leak of issue #31 and no bypass of it was found:
`KERN_PROCARGS`, `KERN_PROCARGS2`, `KERN_PROC.PID`, `proc_pidpath`, `proc_pidinfo` and
`task_for_pid` were each tried against a live host process and each refused. One **High**
finding is open, and it is *not* introduced by this change: a `shared_paths` value can inject
arbitrary SBPL, and a sandboxed command can plant such a value for the user's next run by
writing their shell profile — the same escalation shape #12 closed for `~/.sandme`. Everything
else on the checklist passed.

## Findings

### 1. `shared_paths` injects arbitrary SBPL, and the sandbox can plant it — High

**Vulnerable lines.** `src/profile.rs:158-166` (`expand_path`) and `src/profile.rs:154-158`
(`append_writable_grants`): the path is interpolated into
`(allow file-read* file-write* (subpath "{expanded}"))` with no escaping and no rejection.
`std::fs::canonicalize` neutralises the payload only for a path that exists; a nonexistent one
passes through byte for byte.

**Attack scenario.** Two steps, both available today:

1. A sandboxed command writes `export SANDME_SHARED_PATHS='…payload…'` into the user's shell
   profile. Under the default `shared_paths = ["~/"]`, `~/.zshrc` is writable — only
   `~/.sandme`, `~/Library/Keychains` and the two launchd directories are denied.
2. The user's next `sandme` run loads a profile the sandboxed command wrote. The payload below
   grants unrestricted egress, so SPEC-0001/FR-006 and every destination rule in SPEC-0003 stop
   holding.

This is the escalation SPEC-0002/FR-102 and the `~/.sandme` denial exist to prevent: the policy
must not be writable by what it constrains. `SANDME_SHARED_PATHS` is policy, and it is reachable
through any shell startup file the share includes.

**PoC.** Payload: `/tmp/s31")) (allow network-outbound (remote ip "*:*` — it closes the
`subpath` literal, appends a rule, and leaves `"))` to terminate the injected form, so the
profile still compiles.

```
$ SANDME_SHARED_PATHS='/tmp/s31")) (allow network-outbound (remote ip "*:*' \
    sandme /usr/bin/curl --noproxy '*' -sS -m 6 -o /dev/null -w 'http=%{http_code}\n' https://1.1.1.1/
http=301                                   # direct egress, proxy bypassed

$ SANDME_SHARED_PATHS=/tmp/s31 \
    sandme /usr/bin/nc -vz -w 3 1.1.1.1 443
nc: connectx to 1.1.1.1 port 443 (tcp) failed: Operation not permitted   # control

$ SANDME_SHARED_PATHS='/tmp/s31")) (allow network-outbound (remote ip "*:*' \
    sandme /usr/bin/nc -vz -w 3 1.1.1.1 443
Connection to 1.1.1.1 port 443 [tcp/https] succeeded!
```

Note that a *malformed* payload is loud — `sandbox-exec: syntax error: illegal token`, and
nothing runs — which is why this is easy to miss: only a well-formed payload is silent.

**Requirements violated.** SPEC-0001/FR-006 (egress routed through the proxy), SPEC-0003/FR-201
(the proxy's destination restrictions become unreachable when the profile allows a direct
connection), SPEC-0002/FR-102's premise (a denial the configuration cannot lift).

**Suggested direction.** Escape-or-reject at config load, not at profile generation: a path
containing `"`, `(`, `)`, `\` or a newline is not a path anyone means, so rejecting it with a
`125` and a diagnostic costs nothing and is checkable in one place. Escaping is the weaker
option here — SBPL's quoting rules are not documented well enough to rely on.

### 2. `nc -z` reports success under a profile that denies the connection — Note

`nc -z -w 3 <ip> 443` printed `succeeded!` under a profile that denied the connection; the same
command with `-v` reported `Operation not permitted`, and the kernel logged
`deny(1) network-outbound remote:*:443`. Not a sandme defect — recorded because it is a trap for
anyone verifying egress by hand, and a `-z` check would have produced a false clean result in
this audit.

## Verified clean

Each item below was checked this pass, with the evidence that settled it.

**The change under audit (SPEC-0005).**

- *No bypass of FR-302 by another interface.* Against a live non-platform host process holding
  `HOSTPROC_API_KEY` in its environment, under the new profile: `KERN_PROCARGS` (MIB 1.38),
  `KERN_PROCARGS2` (1.49) and `KERN_PROC.PID` (1.14.1) each return `EPERM`; `proc_pidpath` and
  `proc_pidinfo(PROC_PIDTBSDINFO)` return denied where the old profile answered
  `/private/tmp/s31/holder` and `holder`; `task_for_pid` fails on kernel policy, sandboxed or
  not. Under the old profile the same probes leaked the environment.
- *No name-based bypass of the allowlist.* A numeric-MIB read of a name outside the allowlist is
  refused, so granting the `sysctl.` metadata subtree (name-to-oid lookup) does not widen what
  can be read.
- *Nothing attacker-influenced is formatted into the new rules.* `READABLE_SYSCTL_PREFIXES`,
  `READABLE_SYSCTL_NAMES` and the two literal rules are compile-time constants; no config value,
  `$HOME` or CLI operand reaches them.
- *The new denial cannot be lost.* `(deny process-info*)` is emitted before
  `append_unconditional_denials`' `$HOME` early return, so it survives an unresolvable home —
  asserted by `denies_process_information_without_a_home`.
- *The narrowing is not defeated by rule order.* Measured: a blanket `(allow sysctl-read)`
  anywhere in the profile defeats every later denial of this read, whether the denial precedes
  or follows the grant, which is why the grant itself is narrow. Both new integration tests fail
  against the profile as it was.

**Standing checklist.**

- *Egress is enforced by the profile, not by the environment.* Direct connection to
  `1.1.1.1:443` with `--noproxy '*'` is refused and logged as `deny(1) network-outbound`;
  `network-bind` appears nowhere in the generated profile (`routes_network_only_to_the_proxy`).
- *Plist parsing is bounded.* `executable_name` (`src/app_bundle.rs:105-110`) rejects a value
  containing `/`, so `../../bin/sh` cannot escape the bundle, and the result must be an existing
  file under `Contents/MacOS` before it is used.
- *`$HOME` interpolation.* Reached only through `canonical_home`, which resolves an existing
  directory; a `$HOME` that does not resolve is used verbatim, so the same injection shape as
  finding 1 exists in principle — but `$HOME` is not writable by the sandboxed command in the way
  a shell profile is, and there is no PoC. Capped at the finding above rather than reported twice.
- *Unrestricted `process-exec`.* Inherent to the product: a coding agent must run tools, and a
  writable share plus exec is what makes it usable. Noted, not graded — unchanged by this pass.

## Out of scope

- The proxy's HTTP handling (`src/proxy.rs`), except where the profile's `network-outbound` rule
  interacts with it (checked above).
- CLI parsing (`src/main.rs`, `src/config.rs`) except as the source of the strings in finding 1.
- The profile's other unrestricted operations — `mach-lookup`, `mach-register`, `mach-bootstrap`,
  `iokit-open`, `lsopen`, `ipc-posix-shm*`, `file-read-metadata`. SPEC-0005 declares them a
  non-goal and issue #12 part 4 tracks them; `mach-lookup` was exercised only through
  `task_for_pid` here.
