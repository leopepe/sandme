---
name: security-audit
description: Use when the user runs /security-audit, or asks for a security audit of string injection into the Seatbelt profile or process privilege escalation out of the sandbox. Produces a report; does not fix.
---

# Security audit

Audit the current codebase for two vulnerability classes and write a report to
`.agents/reports/security-audit-<date>.md`. **Report; do not fix.**

The two classes are not arbitrary — they are the two ways sandme's guarantee
can break: the Seatbelt profile in `src/sandbox.rs` is built by formatting
strings, and the sandboxed command is a child process. If a string injection
widens the profile, or the child escapes its intended confinement, the sandbox
guarantees nothing.

## Scope

| Class | Attack surface |
| --- | --- |
| String injection | Anything formatted into the SBPL profile: `generate_profile`, `expand_path`, config-derived paths (`shared_paths`, `HOME`), the proxy address |
| Process privilege escalation | `run` (the `sandbox-exec -p` invocation, env wiring, signal handling), `resolve_app_bundle_executable`, the profile's own process rules |

Out of scope — note and stop, do not audit here: the proxy's HTTP handling
(`src/proxy.rs`) except where it changes what the profile allows; CLI parsing
(`src/main.rs`, `src/config.rs`) except as the source of attacker-influenced
strings.

## 1. Read, then trace

1. Read `src/sandbox.rs`, `src/config.rs`, and `src/error.rs` in full.
2. Read the governing spec in `docs/specs/` for the sandbox requirements
   (default-deny, egress-only-through-proxy) — findings are graded against
   these, not against general paranoia.
3. Trace every value that reaches `writeln!(sbpl, ...)` or
   `Command::new(...).arg(...)` back to its source. Record which sources a
   user or a sandboxed program controls: CLI operands, config file paths,
   `$HOME`, filesystem content (symlinks, app bundles, plist files).

## 2. String-injection checklist

For each string formatted into the profile:

- **SBPL breakout**: can the value contain `"`, `(`, `)`, or a newline? A
  shared path like `/tmp/x")) (allow network-outbound) (subpath "/` would
  close the `subpath` literal and inject a new rule. Check whether
  `expand_path` neutralises this — note it only canonicalises paths that
  exist; a nonexistent malicious path passes through unmodified.
- **Unquoted interpolation of env-derived values**: `HOME` is interpolated
  directly into the `~/Library` rule. Grade the realistic blast radius, not
  the pattern alone.
- **Escape-or-reject**: if injection is possible, the finding states which of
  the two is missing — sandme must either escape SBPL metacharacters or
  reject the input at config time.

## 3. Privilege-escalation checklist

- **Unrestricted `process-exec`**: the profile allows executing any binary
  the child can read. Combined with a writable shared subpath, a sandboxed
  program can write then execute its own files. State whether this violates
  an accepted requirement or is inherent to the product (a coding agent must
  run tools) — the latter is a `Note`, not a `Critical`.
- **TOCTOU on paths**: `expand_path` canonicalises when the profile is
  generated; the kernel evaluates the `subpath` at access time. A symlink
  swapped in between can redirect a granted subpath. Assess whether the
  window is exploitable from inside the sandbox.
- **Trusted plist parsing**: `resolve_app_bundle_executable` extracts
  `CFBundleExecutable` by string slicing. A crafted `Info.plist` in an
  attacker-controlled bundle could supply a value with path traversal
  (`../../`). Check what bounds the `exists()` guard actually provides.
- **Env-based egress**: `HTTP_PROXY`/`HTTPS_PROXY` are advisory; only the
  `network-outbound` rule enforces egress through the proxy. Verify the rule
  allows no other destination, and that `network-bind` stays absent.

## 4. Verify with a PoC before grading High or Critical

A finding is `Critical` or `High` only with a demonstrated exploit against a
`cargo build` binary, e.g.:

```bash
# SBPL injection via a shared path (adjust to the finding)
cargo run -- --share '<crafted-path>' -- sh -c 'curl -s https://example.com'
# Escape check: does egress succeed outside the proxy?
```

Record the exact command and its output in the finding. If you cannot build a
PoC, cap the severity at `Medium` and say why.

## 5. Report format

Write to `.agents/reports/security-audit-<date>.md`:

1. **Summary** — one paragraph, worst finding first.
2. **Findings** — one section each: title, severity
   (`Critical`/`High`/`Medium`/`Low`/`Note`), the vulnerable lines with file
   and line numbers, the attack scenario, the PoC or why there is none, and a
   suggested direction (not a patch).
3. **Verified clean** — each checklist item that was checked and passed, so
   the next audit starts from evidence.
4. **Out of scope** — what was deferred per the scope table.

Every finding cites a requirement ID from `docs/specs/` it violates, or is
explicitly marked as beyond the current spec.
