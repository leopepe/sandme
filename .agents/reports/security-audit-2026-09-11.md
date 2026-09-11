# Security audit — PTY allocation grant (commit `8150669`)

- **Scope of this audit**: the two classes the `security-audit` skill covers —
  string injection into the SBPL profile, and process privilege escalation out
  of the sandbox — applied to the whole profile as changed by commit `8150669`
  ("fix(profile): grant pseudo-terminal allocation (#29)") against `main`
  (`53482e4`). SPEC-0009 governs the change; SPEC-0001/0003 govern the base.
- **Change under audit** (`src/profile.rs`, `generate_profile`): adds
  `(allow file-ioctl (literal "/dev/ptmx"))` and
  `(allow file-read* file-write* (regex #"^/dev/ttys[0-9]+$"))`, removes
  `(allow file-read* file-write* (subpath "/dev/pts"))`.
- **Verified against**: a `cargo build` binary on macOS 26 (Darwin 25.6.0,
  arm64), with live PoCs recorded below.

## Summary

The worst finding is **Medium**: the new slave-device rule
`(allow file-read* file-write* (regex #"^/dev/ttys[0-9]+$"))` matches **every**
pty slave on the host, not only the ones the sandboxed process itself
allocates. A sandboxed process can therefore open a `/dev/ttysNNN` that belongs
to a process **outside** the sandbox (same uid), read bytes off that terminal
(eavesdrop) and write bytes into it. This is demonstrated with a working PoC
against the built binary. It crosses the sandbox boundary and exceeds
SPEC-0009/NFR-901's promise that the grant widens the sandbox "by exactly one
capability — creating a pseudo-terminal — and nothing else."

**It is not a merge blocker.** The decisive escape — command injection into an
outside process, or acquiring a controlling terminal over one — is **blocked**:
that needs `TIOCSTI`/`TIOCSCTTY`, which are `ioctl`s, and the change grants
`file-ioctl` **only** on `/dev/ptmx`, never on the slave devices. The PoC
confirms `TIOCSTI` on a foreign slave is denied. So the residual is a bounded
read/write on other same-uid terminals (racy, partial for reads; reliable for
writes, like `write(1)`), not arbitrary code execution or clean command
injection outside the sandbox. No Critical/High with a working exploit was
found.

The `file-ioctl` grant on `/dev/ptmx` and the `^/dev/ttys[0-9]+$` regex were
each checked for over-reach and found sound (see Verified clean). The
pre-existing `shared_paths` SBPL-injection surface is real but capped at **Low**
because its only source is the trusted config author, who already controls the
entire profile.

## Findings

### F-1 — `ttys` regex grants read/write to pseudo-terminals outside the sandbox (Medium)

- **Requirement**: violates **SPEC-0009/NFR-901** ("THE PTY grant SHALL extend
  to no path beyond the pseudo-terminal multiplexer and the slave devices
  matching `^/dev/ttys[0-9]+$`, and … widens the sandbox by exactly one
  capability — creating a pseudo-terminal — and nothing else"). The rule matches
  the named regex, but the *effect* is broader than "creating a pseudo-terminal":
  it also grants read/write to slaves the process did not create. Also weakens
  **SPEC-0001/SC-002** (a process under sandme is confined).
- **Location**: `src/profile.rs:58` —
  `(allow file-read* file-write* (regex #"^/dev/ttys[0-9]+$"))`.
- **Root cause**: on macOS a pty slave is a global device node `/dev/ttysNNN`
  chowned to the allocating uid at mode `0620` (`crw--w----`). SBPL cannot
  express "only the slave *this process* allocated", so any rule broad enough to
  let `openpty(3)`'s freshly-granted slave be opened also matches every other
  slave the same uid already owns — i.e. the user's other terminal sessions
  (Terminal.app tabs, `ssh` sessions, editors' integrated terminals). Because
  the sandboxed command runs as the same uid, DAC does not stop it; the profile
  rule is the only gate, and it opens all of them.
- **Attack scenario**: a malicious or compromised coding agent running under
  sandme enumerates `/dev/ttys*` (metadata reads are globally allowed), opens a
  slave belonging to one of the user's other terminals, and (a) reads bytes the
  user types or that appear there — capturing secrets such as pasted tokens or a
  passphrase — and (b) writes arbitrary bytes/escape sequences into that
  terminal. The read is racy in practice (it competes with the terminal's real
  reader); the write is reliable.
- **What is NOT possible** (bounds the severity): command injection into the
  outside process's shell (`TIOCSTI`) and taking the terminal as a controlling
  terminal (`TIOCSCTTY`) both require `ioctl`, which the change grants only on
  `/dev/ptmx`. The PoC confirms `TIOCSTI` on a foreign slave is denied. There is
  no path from this rule to running a command outside the sandbox.
- **PoC** (against the `cargo build` binary and, for attribution, `sandbox-exec`
  directly). An out-of-sandbox Python process allocates a pty and holds its
  master; its slave is `/dev/ttys006`. Then, inside sandme's real generated
  profile (defaults, `gui_mode` off):

  ```
  $ ./target/debug/sandme "/usr/bin/python3 ~/sandme_poc_probe.py"
  OPEN /dev/ttys006: OK (fd=3)
  READ foreign tty: b'SECRET-TYPED-AT-FOREIGN-TERMINAL\n'
  WRITE foreign tty: OK
  TIOCSTI ioctl: DENIED ([Errno 1] Operation not permitted)
  # the out-of-sandbox master then received: INJECTED-BY-SANDBOXED-PROCESS
  ```

  Attribution to the new rule — same probe under a main-equivalent profile (no
  `ttys` rule) vs. this change's profile:

  ```
  --- MAIN-equivalent profile (no ttys rule) ---
  OPEN /dev/ttys006: DENIED ([Errno 1] Operation not permitted: '/dev/ttys006')
  --- THIS CHANGE profile (ttys rule) ---
  OPEN /dev/ttys006: OK (fd=3)
  READ foreign tty: b'SECRET-TYPED-AT-FOREIGN-TERMINAL\n'
  WRITE foreign tty: OK
  TIOCSTI ioctl: DENIED ([Errno 1] Operation not permitted)
  ```

  So the capability is newly introduced by this commit (denied on `main`, where
  no rule matched the slave and PTY allocation failed outright), and the
  boundary crossing is confirmed: the sandboxed process read a secret the
  out-of-sandbox holder placed on the line, and its write reached the
  out-of-sandbox master.
- **Severity justification**: a demonstrated cross-boundary read + write is a
  real confinement breach, so it is more than a Note; but it yields no code
  execution and no clean command injection outside the sandbox (the decisive
  `ioctl` path is blocked), and reads are racy/partial, so it is not High.
  **Medium.**
- **Suggested direction** (not a patch): this residual is largely inherent to
  supporting PTYs on macOS — SBPL has no predicate for "a device this process
  created", so the slave grant cannot be narrowed to self-allocated nodes
  without a different mechanism (e.g. a helper that opens the slave before the
  sandbox is entered and passes the fd in, or gating the whole PTY grant behind
  an opt-in). At minimum, correct NFR-901's "and nothing else" wording to state
  the honest residual (read/write of same-uid slaves), and record in the spec
  that the mitigation relied upon is the withheld `file-ioctl` on the slave
  devices — which is what actually keeps the escape (TIOCSTI/TIOCSCTTY) closed.
  The `file-ioctl`-on-`/dev/ptmx`-only split is the right call and should be
  called out as load-bearing, not incidental.

### F-2 — `shared_paths` / `$HOME` interpolate unescaped into SBPL (Low, pre-existing, not this change)

- **Requirement**: beyond the current spec's threat model (the sandboxed process
  cannot reach this input); noted as defense-in-depth against SPEC-0001/FR-004.
- **Location**: `src/profile.rs:105-108` (`append_writable_grants`, the
  `writeln!` of `(subpath "{expanded}")`) and `expand_path` at
  `src/profile.rs:167-176`; plus `$HOME` at `src/profile.rs:117-119` and
  `:145`/`:150`.
- **Mechanism**: `expand_path` canonicalises a path only when it *exists*; a
  **nonexistent** entry passes through unmodified, metacharacters and all. A
  `shared_paths` value that does not exist on disk can therefore close the
  `subpath` literal and inject arbitrary SBPL.
- **PoC** (mechanism, replicating `expand_path` verbatim):

  ```
  input  : /nonexistent-xyz")) (allow network-outbound) (subpath "/
  emitted: (allow file-read* file-write* (subpath "/nonexistent-xyz")) (allow network-outbound) (subpath "/"))
  ```

  The injected `(allow network-outbound)` would grant unrestricted egress,
  defeating SPEC-0003.
- **Why Low, not High**: the sources of `shared_paths` and `$HOME` are the
  config file (`~/.sandme/config.toml`) and the parent process's environment —
  both controlled by the **user who runs sandme**, never by the sandboxed
  command. The sandboxed command cannot write `~/.sandme` (denied at
  `profile.rs:96`/`:149-151`) and cannot alter its parent's environment. The
  config author already sets the whole profile (they can write
  `shared_paths = ["/"]` outright), so injection grants them nothing they did
  not already have: the blast radius is self-inflicted, not a
  sandboxed-process privilege escalation. This is unchanged by the commit under
  audit.
- **Suggested direction**: reject config-time values containing `"`, `(`, `)` or
  a newline (or escape them), so a copy-pasted or programmatically generated
  config cannot silently widen the profile. Belongs to config validation, not
  to this PTY change.

## Verified clean

Each item below was checked against the change and passed; evidence is recorded
so the next audit can start from it.

- **`file-ioctl` on `/dev/ptmx` does not open extra escape** — `file-ioctl` is
  coarse (it permits *all* ioctls on the named vnode), but `/dev/ptmx` hands out
  a **fresh master** owned by the opener on every open, so any ioctl issued
  through this grant acts only on the process's own pty pair. The dangerous
  ioctls (`TIOCSTI` input injection, `TIOCSCTTY` controlling-terminal takeover)
  matter on a **slave** another process treats as its terminal — and the change
  grants **no** `file-ioctl` on the `ttys` slaves. PoC confirms `TIOCSTI` on a
  foreign slave is denied.
- **The `^/dev/ttys[0-9]+$` regex matches nothing unintended** — anchored at
  both ends, body is digits only; it cannot contain a `/`, so no path traversal
  and no escape to a directory. It matches exactly the BSD pty slave nodes
  (`/dev/ttys000…`) plus the root-owned console ttys `/dev/ttys0..2`; none of
  these adds any capability beyond F-1's read/write, and the Rust string
  `#"^/dev/ttys[0-9]+$"` survives escaping into the profile intact (asserted by
  `grants_pseudo_terminal_allocation`).
- **No cross-sandbox signalling or controlling-terminal takeover** —
  `TIOCSCTTY` is an `ioctl` on the slave → denied; and the profile's
  `(allow signal (target self))` restricts signals to the process's own tree, so
  a sandboxed process cannot signal an outside process via job control on a
  shared terminal either.
- **`/dev/pts` removal is safe** — the removed rule matched nothing on macOS
  (the path does not exist), so its removal narrows nothing a real host relied
  on (SPEC-0009/SC-902); confirmed the generated profile now contains no
  `/dev/pts` rule.
- **Egress rule unchanged and still closed** — the only `network-outbound` rule
  is `(remote ip "localhost:{port}")` and no `network-bind` rule exists
  (`profile.rs:70-74`; test `routes_network_only_to_the_proxy`). The PTY change
  adds no network reach, satisfying the network half of NFR-901.
- **`resolve_app_bundle_executable` plist slicing is bounded** —
  `executable_name` (`app_bundle.rs:106-111`) rejects any `CFBundleExecutable`
  value containing `/`, so a crafted `Info.plist` cannot traverse out of
  `Contents/MacOS`; the result is additionally gated by `main.is_file()`
  (`app_bundle.rs:73`). Not touched by this change; re-confirmed.
- **Quality gate green on the branch after adding this report** —
  `cargo fmt --check` clean, `cargo clippy --all-targets` 0 warnings,
  `cargo build` 0 warnings, `cargo test` 38 passed / 0 failed (including
  `allocates_a_pseudo_terminal` and `grants_pseudo_terminal_allocation`),
  `cargo doc --no-deps` 0 warnings.

## Out of scope

Per the skill's scope table, the following were noted and not audited here:

- **Proxy HTTP handling** (`src/proxy.rs`, `src/egress.rs`,
  `src/credential.rs`) — except that it was confirmed the PTY change alters
  nothing the proxy allows (no new `network-*` rule).
- **CLI / config parsing** (`src/main.rs`, `src/config.rs`) — treated only as
  the source of the attacker-influenced strings assessed in F-2, not audited as
  a surface in itself.
- **Unrestricted `process-exec`** (`profile.rs:44`) — inherent to the product (a
  coding agent must run tools); a `Note`, not a finding, and unchanged by this
  commit.
- **TOCTOU on `shared_paths` symlinks** — pre-existing property of
  `expand_path`/kernel-time evaluation, unchanged by this commit; not
  re-assessed here.
