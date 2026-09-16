# sandme

`sandme` runs a command — your IDE, your coding agent, or anything else — inside a kernel sandbox,
with its network egress routed through a proxy `sandme` starts and stops alongside it. On macOS the
sandbox is
[Seatbelt](https://developer.apple.com/library/archive/documentation/Darwin/Reference/ManPages/man7/sandbox.7.html),
applied by `sandbox-exec`. On Linux it is
[Landlock](https://docs.kernel.org/userspace-api/landlock.html), which the command applies to
itself just before it execs.

Everything is denied by default. The command gets read+write access to the paths you share, and
one network destination: the proxy. Child processes inherit the same sandbox, so an agent that
shells out cannot step outside it.

Linux needs kernel 6.7 or newer — Landlock ABI v4, the first version that can restrict outbound TCP
by port. Below that `sandme` refuses to start rather than confine the filesystem and leave egress
open. Landlock reaches the filesystem and outbound TCP; UDP and raw sockets stay outside it.

`sandme`'s lifetime is the command's lifetime — no daemon, nothing left running.

## Install

**A release binary.** Every tagged release publishes a tarball per target —
`aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu` — with a matching
`.sha256`, on the [releases page](https://github.com/leopepe/sandme/releases):

```shell
shasum -a 256 -c sandme-v0.1.0-aarch64-apple-darwin.tar.gz.sha256
tar xzf sandme-v0.1.0-aarch64-apple-darwin.tar.gz
cp sandme-v0.1.0-aarch64-apple-darwin/sandme /usr/local/bin/
```

The binaries are not signed or notarised. On macOS a downloaded tarball carries a quarantine
attribute, and Gatekeeper refuses the binary — the dialog says macOS cannot check it for malicious
software, which reads like a broken tool and is not one. Clear the attribute once:

```shell
xattr -d com.apple.quarantine /usr/local/bin/sandme
```

**From source.**

```shell
cargo build --release
cp target/release/sandme /usr/local/bin/
```

## Usage

```
sandme <command> [args...]
```

There are two forms, and the difference matters.

**Multiple operands — direct exec.** The first word is the program, the rest are its arguments,
passed through unshelled and unquoted. No shell is involved.

```shell
sandme ls -la ~/Workspace
sandme cargo test
```

**A single quoted operand — routed through `/bin/bash -c`.** Use this when you want shell features:
pipes, redirection, globbing, `&&`, variable expansion, and process substitution — `cat <(echo hi)`
works in this form. (`diff <(a) <(b)` additionally needs `gui_mode`, because macOS `diff` copies
non-seekable input to a temp file.)

```shell
sandme 'ls -la ~/Workspace | grep rust'
sandme 'cargo build 2>&1 | tee build.log'
```

Use `--` when the command has its own options that would otherwise be read as `sandme`'s:

```shell
sandme -- curl -sS https://example.com/
```

### Exit status

`sandme` follows the exec-wrapper convention of `env(1)` and `timeout(1)`, so a script can tell
a failure of `sandme`'s from a failure of the command it wrapped:

| Status | Meaning |
| --- | --- |
| `0`–`124` | The command's own exit status, passed through unchanged |
| `125` | `sandme` itself failed — unreadable or malformed config, the proxy could not bind, the sandbox could not be started. The command did not run. |
| `126` | The command was found but is not executable |
| `127` | The command was not found |
| `128+n` | The command was killed by signal `n` (Ctrl-C gives `130`) |

Every status `sandme` produces for itself comes with a `sandme: ` line on stderr, and `sandme`
prefixes nothing it did not write. That is the reliable signal: a command is free to exit `125`
on its own account, and `sandme` passes that through untouched rather than rewriting it.

## Configuration

`sandme` reads `~/.sandme/config.toml`. Every key has a matching environment variable, and the
environment always wins.

| Key | Environment variable | Type | Default | What it does |
| --- | --- | --- | --- | --- |
| `shared_paths` | `SANDME_SHARED_PATHS` | array of strings (env: comma-separated) | the current working directory | Paths the sandboxed command may **read and write**. `~/` expands to your home directory; symlinks are resolved. Set `["~/"]` to share your whole home. |
| `read_only_paths` | `SANDME_READ_ONLY_PATHS` | array of strings (env: comma-separated) | empty | Paths the sandboxed command may **read and execute** but **not write** — a toolchain prefix (a Homebrew directory, a language runtime) whose binaries must run while writes stay confined to `shared_paths`. `~/` expands and symlinks are resolved, as for `shared_paths`. |
| `proxy` | `SANDME_PROXY` | boolean (env: `1` or `true`) | `true` | Whether to start the egress proxy and route the command through it. Set `false` to run with **no egress on macOS, and no outbound TCP on Linux** (see the note below). |
| `proxy_port` | `SANDME_PROXY_PORT` | integer | `0` | Loopback port the egress proxy listens on. `0` (the default) lets the OS pick a free ephemeral port, so parallel `sandme` runs never collide. Set a non-zero value to pin a fixed, predictable port. |
| `gui_mode` | `SANDME_GUI_MODE` | boolean (env: `1` or `true`) | `false` | Also grants read+write to the per-user state and scratch directories: `~/Library` and `$TMPDIR` on macOS, the XDG config, cache, data and runtime directories on Linux. GUI apps and most editors need this for their state, caches and scratch space. |
| `allow_private_egress` | `SANDME_ALLOW_PRIVATE_EGRESS` | boolean (env: `1` or `true`) | `false` | Lets the proxy relay to your own machine and network — loopback, RFC1918, link-local. Needed for a locally hosted service (a local model server, a dev API); see [Network](#what-the-sandbox-allows) for what it re-opens. |

No config file is required — without one you get the defaults. A malformed file is reported
rather than ignored.

> **The default shares the directory you run `sandme` in.** `shared_paths` defaults to the current
> working directory, so out of the box the sandboxed command reads and writes that directory and
> nothing else under `~` — `~/.ssh` and `~/.aws` are not exposed. To share more, name it in
> `shared_paths`; `["~/"]` restores the old whole-home behaviour.

> **`proxy = false` disables all network remotes**, not just the HTTP proxy: with no proxy running
> the command gets no `HTTP(S)_PROXY` and no git-over-SSH tunnel, so both HTTP(S) and git-over-SSH
> remotes fail. That is the point — a no-network run — and an SSH git failure under `proxy = false`
> is expected, not a bug. On macOS the sandbox then denies all outbound network; on Linux it denies
> all outbound TCP, which is as far as Landlock reaches.

### A starting config

```toml
# ~/.sandme/config.toml
shared_paths = [
  "~/Workspace",           # your projects — narrow this to taste
]
read_only_paths = [
  "/opt/homebrew",         # Homebrew toolchain (Apple silicon; use /usr/local on Intel) —
                           # readable and runnable, but not writable
]
gui_mode = true            # editors need ~/Library and scratch space
# proxy_port defaults to 0 (an OS-chosen ephemeral port); set it only to pin a fixed port.
```

There is a fuller, commented version in [`examples/config.toml`](examples/config.toml).

## What the sandbox allows

**Filesystem — macOS.** Read-only access to the system runtime — `/usr`, `/bin`, `/sbin`,
`/System`, `/Library`, `/Applications`, `/private/etc`. Read+write to everything in `shared_paths`,
plus — only with `gui_mode` — `~/Library` and your per-user temp directory (`$TMPDIR`). `gui_mode`
does **not** open the world-shared `/private/tmp` or all of `/private/var/folders`, only the temp
directory macOS gives your own session. Everything else is denied for both reading and writing.

Cross-application AppleEvents (`osascript`-style scripting of other apps) are denied outright:
the sandbox has no reason to drive another application.

Three directories are denied outright, and no setting grants them back:

| Denied always | Why |
| --- | --- |
| `~/Library/LaunchAgents`, `~/Library/LaunchDaemons` | A plist written here is run by launchd at your next login — **outside** the sandbox. A writable one turns any share into a persistence escape. |
| `~/Library/Keychains` | Your login keychain. Apps that store credentials through Keychain Services are unaffected: `securityd` reads the files, not the sandboxed process. |

The rules are emitted last in the profile, after every grant, because Seatbelt resolves a path
against the last rule that matches it. `shared_paths = ["~/"]` does not lift them. These denials,
and the AppleEvents one above, are macOS-only — Landlock has no deny primitive to express them.

**Filesystem — Linux.** Read+execute on the system runtime — `/usr`, `/bin`, `/sbin`, `/lib`,
`/lib64`, `/opt` — and read on `/etc` and the random devices. Read+write to everything in
`shared_paths`, to `/dev/null` and `/dev/tty`, and — only with `gui_mode` — to `$XDG_CONFIG_HOME`,
`$XDG_CACHE_HOME` and `$XDG_DATA_HOME` (falling back to `~/.config`, `~/.cache` and
`~/.local/share`) and to `$XDG_RUNTIME_DIR` (falling back to `/tmp`). Everything else is denied.

`/proc` and `/sys` are granted to nothing. Landlock is allow-only with no deny primitive, so the
one way to keep another process's `/proc/<pid>/environ` out of reach is never to grant `/proc` at
all. For the same reason a `shared_paths` entry that would re-admit either tree — `/` itself, or an
ancestor of one of them — is refused rather than honoured.

**Pseudo-terminals.** A command may allocate one, so an editor's integrated terminal and its
login-shell environment loading work ([#29](https://github.com/leopepe/sandme/issues/29)). This
is a real capability — a PTY is a kernel object the command creates — and needs no `gui_mode`. On
Linux that is `/dev/ptmx` and the `/dev/pts` subtree, granted read+write for the same reason.

**Network.** TCP to the proxy port, and nothing else. On macOS direct HTTP, DNS, raw sockets, ICMP
and listening sockets are all denied. On Linux the rule is a port number with no address — Landlock
has no address predicate — so the grant is outbound TCP to that port on any host, and UDP, raw
sockets and listening sockets are outside its reach. `sandme` sets `HTTP_PROXY`, `HTTPS_PROXY` and
their lowercase forms in the command's environment, so ordinary HTTP clients use the proxy without
you configuring anything. HTTPS works through `CONNECT`.

The proxy refuses to relay to the destinations the sandbox itself blocks: loopback
(`127.0.0.0/8`, `::1`), RFC1918 (`10/8`, `172.16/12`, `192.168/16`), link-local (`169.254/16`,
which includes the cloud metadata address `169.254.169.254`, and `fe80::/10`), unique-local
(`fc00::/7`) and the unspecified addresses. A request for one of those gets `403` and a line on
stderr saying so — hostnames included, so `localtest.me` and friends are refused too. Without
this the proxy would reach every host-local and LAN service on your behalf, and the profile's
network rule would describe the route rather than the confinement
([#15](https://github.com/leopepe/sandme/issues/15)).

If you are running a coding agent against a **local model server** — Ollama on
`127.0.0.1:11434`, or any locally hosted API — turn that off explicitly:

```shell
SANDME_ALLOW_PRIVATE_EGRESS=1 SANDME_GUI_MODE=1 sandme claude
```

That re-opens the whole of your machine and LAN to the sandboxed command, which is the point of
the setting and also its cost. It does not affect who may use the proxy.

Each run's proxy also requires a credential, generated per invocation and published to the
command in `HTTP_PROXY`/`HTTPS_PROXY`. Ordinary HTTP clients read it from there and send it, so
nothing needs configuring; a request from any other local process gets `407`. Beyond
destinations and that credential, the proxy does not inspect or filter what it relays.

## Recipes

Each of these is verified against the current build on macOS 26 (Apple silicon). The Homebrew,
Neovim and Zed recipes name macOS paths and macOS behaviour — the `/opt/homebrew` prefix, the
app-bundle redirect. The shape carries over to Linux; the prefixes do not.

### Simple commands

The defaults are enough:

```shell
sandme ls -la ~/Workspace
sandme -- curl -sS https://example.com/
sandme 'echo hello-world | tr - " "'
```

### Anything installed by Homebrew

Homebrew lives in `/opt/homebrew` on Apple silicon, which is **not** in the base read-only set. A
brew-installed binary cannot load its own dylibs until you share that prefix:

```shell
# fails: dyld: Library not loaded: /opt/homebrew/opt/pcre2/lib/libpcre2-8.0.dylib
sandme 'fish -c "echo hi"'

# works
SANDME_SHARED_PATHS="$HOME,/opt/homebrew" sandme 'fish -c "echo hi"'
```

Put `/opt/homebrew` in `read_only_paths` once and forget about it: a toolchain prefix has to be
readable and runnable, not writable, and `read_only_paths` grants exactly that. `shared_paths`
works too, at the cost of granting **write** access to the prefix. On Intel Macs Homebrew uses
`/usr/local`, which is already readable via `/usr`. If you use Nix, add `/nix` too.

### Neovim

Neovim needs its Homebrew prefix, its own config and state under `~`, and scratch space:

```toml
# ~/.sandme/config.toml
shared_paths = ["~/", "/opt/homebrew"]
gui_mode = true
```

```shell
sandme nvim ~/Workspace/my-project
```

Or without a config file:

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme nvim ~/Workspace/my-project
```

Without `gui_mode` Neovim starts but prints `tempdir create failed: operation not permitted` and
loses swap files, undo history and `:terminal`.

To confine it to one project instead of all of `~`, share the project plus Neovim's own
directories:

```shell
SANDME_GUI_MODE=1 \
SANDME_SHARED_PATHS="~/Workspace/my-project,/opt/homebrew,~/.config/nvim,~/.local/share/nvim,~/.local/state/nvim,~/.cache/nvim" \
  sandme nvim ~/Workspace/my-project
```

System `vim` in `/usr/bin` needs none of this — `sandme vim <file>` works with the defaults.

### Zed

Zed needs `gui_mode`; the command name is enough:

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme zed ~/Workspace/my-project
```

Zed launches, runs sandboxed, and Ctrl-C in the launching terminal shuts it and the proxy down.
The quoted form (`sandme 'zed ~/Workspace/my-project'`) and the wrapper's absolute path work the
same way.

`zed` on `PATH` is a CLI wrapper that asks LaunchServices to open `Zed.app`, and LaunchServices is
blocked inside the sandbox. `sandme` looks the command up on `PATH`, notices it points into an
`.app`, and runs the bundle's own executable instead. The same happens for any other IDE shipped
as a `.app` with a CLI wrapper.

The wrapper and the bundle executable do not take the same options — Zed's wrapper has `--wait`,
`--new` and `--version`, its bundle executable has `--diff` and `--user-data-dir`. Paths work on
both, so opening a project is unaffected, but wrapper-only flags are not: `sandme zed --version`
now reports `unexpected argument`, and `EDITOR='zed --wait'` will not block. Pass paths, not
wrapper flags.

One case is deliberately left alone: a wrapper buried inside a compound shell string
(`sandme 'cd ~/proj && zed .'`). Only the first word of a quoted command is redirected, because
working out which word of a compound command is the program means guessing, and a wrong guess
would run something you did not ask for. Name the app first, or use the multi-operand form.

### Coding agents

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme claude
```

The agent's HTTP calls go through the proxy — with the caveats in the next section.

Narrowing the share to the project alone does not work today:

```shell
# fails: error: An internal error occurred (EPERM)
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="~/Workspace/my-project,/opt/homebrew" \
  sandme claude
```

`claude` is a native binary that finds its own home through the passwd database rather than
`$HOME`, and it reads and rewrites state next to `~/.claude.json`. Sharing `~/.claude` and
`~/.claude.json` explicitly is not enough. Until `shared_paths` can express that shape
([#10](https://github.com/leopepe/sandme/issues/10)), share `$HOME` and rely on the network
confinement rather than the filesystem confinement.

### Several invocations at once

Parallel proxied runs work out of the box: `proxy_port` defaults to `0`, so each invocation binds
its own OS-chosen ephemeral port and none collide.

```shell
sandme cargo test &
sandme cargo clippy &
```

To pin a fixed, predictable port for one run, set `proxy_port` explicitly (only one run may hold a
given port at a time):

```shell
SANDME_PROXY_PORT=9000 sandme cargo test
```

## Known limitations

Read these before trusting the sandbox with something hostile.

| | Issue |
| --- | --- |
| `mach-lookup` is still allowed with no service allowlist. Cross-application AppleEvents are now denied (`appleevent-send`), but narrowing the mach-lookup family to an allowlist is deferred — no escape has been demonstrated, but the channel is not fully closed. | [#12](https://github.com/leopepe/sandme/issues/12) |
| An app-bundle CLI wrapper is only redirected when it is the first word of the command; inside a compound shell string (`sandme 'cd ~/proj && zed .'`) it is left to the shell and fails. | [#13](https://github.com/leopepe/sandme/issues/13) |
| Redirecting to the bundle executable loses the wrapper's own flags (`zed --wait`, `--version`) — the two binaries have different CLIs. Paths are unaffected. | [#13](https://github.com/leopepe/sandme/issues/13) |
| `diff <(a) <(b)` needs `gui_mode`, because macOS `diff` copies non-seekable process-substitution input to a temp file under `$TMPDIR`. (Process substitution itself works in the single-operand form since it is routed through `/bin/bash`.) | [#12](https://github.com/leopepe/sandme/issues/12) |
| The proxy runs unsandboxed. It now refuses loopback, RFC1918 and link-local destinations and requires a per-run credential, but there is no destination allowlist, and `allow_private_egress` re-opens all of it at once. | [#15](https://github.com/leopepe/sandme/issues/15) |
| macOS `/usr/bin/git` is an `xcrun` shim that caches into `$TMPDIR` under `/private/var/folders`. Without `gui_mode` that write is denied and git fails with `Operation not permitted` — the message names `xcrun_db`, not sandme. Run with `gui_mode` on, which grants the temp directories. | [#29](https://github.com/leopepe/sandme/issues/29) |
| git cannot reach a real remote: SSH remotes (port 22) are denied by design — egress goes only through the proxy — and HTTPS remotes fall through to a username prompt with no terminal to answer it. | [#29](https://github.com/leopepe/sandme/issues/29) |

## Development

```shell
cargo build            # build
cargo test             # unit + integration tests
cargo clippy --all-targets
cargo fmt
```

`AGENTS.md` describes the working agreement; `docs/specs/` holds the requirements, `docs/adrs/`
the structural decisions, and `docs/guidelines/` the rules that apply across changes.
