# Security audit — 2026-09-10

> **Note (2026-09-16)**: source paths below reflect the module layout at the time of this
> audit. The sandbox modules have since been split — `src/profile.rs` is now
> `src/sandbox/seatbelt/profile.rs`, `src/sandbox.rs` is now `src/sandbox/mod.rs`, and
> `src/app_bundle.rs` is now `src/sandbox/seatbelt/app_bundle.rs`. Findings and line
> references are left as recorded.

Scope: string injection into the SBPL profile, and process privilege escalation
out of the sandbox. Audited against the change on branch
`fix/process-information-isolation` (SPEC-0005: process-information isolation)
and the surrounding sandbox code. Report only — nothing fixed.

## Summary

The change under review is clean: it narrows two grants and adds three denials,
introduces no new interpolation into the profile, and no new process rule. Its
own security property holds under a PoC (below, Verified clean).

The audit's one actionable finding **predates this change**: `shared_paths` (and
`$HOME`) are interpolated into the profile without escaping, so a crafted value
closes the `subpath` literal and injects arbitrary SBPL. A demonstrated exploit
grants a sandboxed command **direct network egress, bypassing the proxy** —
`connect=0 http=301` to a public IP where the baseline gets `connect=7`. Rated
**High**. It is the injection class the new SPEC-0005 already records as a
non-goal and points at [#41](https://github.com/leopepe/sandme/issues/41); this
audit supplies the PoC and confirms the new denials do **not** close it.

Everything else in scope passed: `network-bind` is absent, `network-outbound`
reaches only the proxy, the plist parser rejects traversal, the config file is
unwritable from inside, and the new process-information denials refuse a host
process's environment under a PoC.

---

## Findings

### F1 — `shared_paths` and `$HOME` inject arbitrary SBPL, bypassing proxy egress

**Severity: High** · violates **SPEC-0001/FR-006** (the sandboxed command's
egress is routed through the proxy), restated as still-binding in
`docs/specs/0003-proxy-egress-restrictions.md:41`.

**Vulnerable code.** `src/profile.rs:88-95`, `append_writable_grants`:

```rust
for path in &config.shared_paths {
    let expanded = expand_path(path);
    let _ = writeln!(
        sbpl,
        "(allow file-read* file-write* (subpath \"{expanded}\"))"
    );
}
```

`expanded` is written between `"` delimiters with no escaping. `expand_path`
(`src/profile.rs:178`) only runs `std::fs::canonicalize`, which returns the
input unchanged for a path that does not exist — and macOS filenames may
contain `"`, `(` and `)`, so even an existing path is not neutralised. The same
pattern interpolates `$HOME` at `src/profile.rs:105` and `:155-160`.

The value's source is attacker-influenced. `shared_paths` comes from
`SANDME_SHARED_PATHS` or `~/.sandme/config.toml` (`src/config.rs:107`). A
sandboxed command cannot write the config file (denied — see Verified clean),
but under the **default `shared_paths = ["~/"]`** (`src/config.rs:60`) it can
write `~/.zshrc`, and a line there sets the environment of the user's next
login shell — and so of the next `sandme` run:

```
$ sandme sh -c 'echo "export SANDME_SHARED_PATHS=/" >> ~/.zshrc'
   → host ~/.zshrc now contains: export SANDME_SHARED_PATHS=/
```

This is the persistence shape #12 closed for `~/.sandme`; the environment
variable is a second door into the same room.

**PoC** (against a `cargo build` binary, macOS 26 / Darwin 25.6.0):

```bash
INJ='/tmp/nope")) (allow network-outbound (remote ip "*:*")) (allow file-read* (subpath "/'
PROBE='curl -sS --noproxy "*" -o /dev/null --connect-timeout 8 \
       -w "connect=%{exitcode} http=%{http_code}\n" http://1.1.1.1/'

# baseline, no injection:
env HOME=$H SANDME_PROXY_PORT=0 ./target/debug/sandme sh -c "$PROBE"
#   → connect=7 http=000        (socket refused — egress denied)

# injection:
env HOME=$H SANDME_PROXY_PORT=0 SANDME_SHARED_PATHS="$INJ" ./target/debug/sandme sh -c "$PROBE"
#   → connect=0 http=301        (TCP opened to a public IP, HTTP answered — proxy bypassed)
```

The exploit succeeds identically when `$INJ` is an **existing** directory whose
name carries the metacharacters, so the "canonicalize only touches real paths"
mitigation does not bound it: `connect=0 http=301` in that case too.

Injecting `(allow network-outbound (remote ip "*:*"))` defeats FR-006 directly.
The same primitive can inject any rule — `(allow process-info-pidinfo)` reopens
the very leak this branch closes, which is why SPEC-0005's
`denies_reading_another_process_environment_under_an_injected_grant` exists and
why FR-303/FR-304 blunt the *simplest* payload without ending the class.

**Direction.** Escape-or-reject at config load, before any value reaches the
profile. Either reject a `shared_paths` entry or `$HOME` containing `"`, `(`,
`)`, `\` or a newline with a `SandmeError` at load time, or escape those
characters for SBPL. Rejecting is simpler to reason about and matches
default-deny; escaping keeps legitimate-but-odd paths working. This is #41;
the audit confirms it is reachable and unmediated.

### F2 — unrestricted `process-exec` plus a writable share is write-then-execute

**Severity: Note** · no requirement violated — inherent to the product.

`src/profile.rs:31` grants `(allow process-exec)` unconditionally, and any
`shared_paths` entry is granted `file-write*`. A sandboxed command can write a
binary or script into a shared path and execute it. This is not an escape:
whatever it executes runs under the same profile, with the same egress and
filesystem confinement. SPEC-0001's Goals require running an untrusted coding
agent, which must compile and run tools, so a general exec grant is by design.
Recorded so the next audit does not re-derive it. No PoC built (would
demonstrate intended behaviour).

### F3 — `$HOME` interpolation shares F1's class through a narrower door

**Severity: Low** · same requirement as F1.

`$HOME` reaches the `~/Library` grant (`src/profile.rs:105`) and the home-relative
denials (`:155-160`) unescaped, via `canonical_home` (`:170`), which likewise
only canonicalises. Exploiting it means controlling `$HOME` for a `sandme` run —
reachable through the same `~/.zshrc` vector as F1 (`export HOME=/path/to/crafted`).
Folded into F1's fix: escape-or-reject must cover `$HOME`, not only
`shared_paths`. Not separately PoC'd; the interpolation is identical to F1's,
which is proven.

---

## Verified clean

Each checked against the branch binary or the source cited.

- **Proxy egress rule is the only network grant.** `src/profile.rs:56-59` emits
  `(allow network-outbound (remote ip "localhost:{port}"))` and nothing else;
  `network-bind` and `network-inbound` appear nowhere. Asserted by
  `profile::tests` at `src/profile.rs:245-246`. Absent an injected rule (F1),
  the baseline PoC gets `connect=7` — no egress.
- **Proxy address cannot inject.** `proxy.port()` is a `u16` formatted as digits
  (`src/profile.rs:58`); no attacker-controlled string reaches that line.
- **Plist `CFBundleExecutable` traversal is rejected.**
  `app_bundle::executable_name` (`src/app_bundle.rs:106-110`) returns `None` for
  a value containing `/`, and the result must be a real file under
  `Contents/MacOS` (`:72-74`). `executable_name("../../../bin/sh") == None` is
  asserted at `src/app_bundle.rs:145`. A crafted bundle cannot redirect sandme
  outside itself.
- **The config file is unwritable from inside the sandbox.** `.sandme` is in
  `DENIED_HOME_DIRECTORIES` (`src/profile.rs:80`), denied write after every
  grant (`:159-160`). PoC: `mkdir ~/.sandme` inside the sandbox →
  `Operation not permitted`, host file absent. This is what forces F1's attacker
  onto the slower `~/.zshrc` env vector rather than writing config directly.
- **`HTTP_PROXY`/`HTTPS_PROXY` are advisory only.** `src/sandbox.rs:117-120`
  sets them for convenience; egress is enforced by the `network-outbound` rule,
  not the variables. Overriding them (`--noproxy '*'`, as the PoCs do) does not
  widen egress — the baseline still gets `connect=7`.
- **The change's own property holds.** Under the new profile, a sandboxed probe
  reading `KERN_PROCARGS2` for a host process holding a marker gets
  `Operation not permitted` and the marker does not appear; the same holds when
  an unscoped `(allow process-info-pidinfo)` + `(allow sysctl-read)` is injected
  through `shared_paths` (the FR-303/FR-304 denials outrank it). `curl` through
  the proxy still returns `http=200`, and a command still reads its own child's
  process information.

---

## Out of scope

Per the scope table, deferred and **not** audited here:

- **The proxy's HTTP handling** (`src/proxy.rs`) beyond its effect on the
  profile. SPEC-0003 already covers the pivot and open-relay concerns; the
  credential and destination checks live there.
- **CLI parsing** (`src/main.rs`, `src/config.rs`) except as the source of the
  attacker-influenced strings traced in F1/F3.
- **The other unrestricted operation grants** in the template — `mach-lookup`,
  `mach-register`, `mach-bootstrap`, `lsopen`, `iokit-open`,
  `ipc-posix-shm*` (`src/profile.rs:36-41`). Raised by issue #12 part 4; a
  separate audit, and named as a non-goal in SPEC-0005.
