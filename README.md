# sandme

`sandme` runs a command — your IDE, your coding agent, or anything else — inside a macOS
[Seatbelt](https://developer.apple.com/library/archive/documentation/Darwin/Reference/ManPages/man7/sandbox.7.html)
sandbox, with its network egress routed through a proxy `sandme` starts and stops alongside it.

Everything is denied by default. The command gets read+write access to the paths you share, and
one network destination: the proxy. Child processes inherit the same sandbox, so an agent that
shells out cannot step outside it.

macOS only. `sandme`'s lifetime is the command's lifetime — no daemon, nothing left running.

## Install

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

**A single quoted operand — routed through `/bin/sh -c`.** Use this when you want shell features:
pipes, redirection, globbing, `&&`, variable expansion. Note that `/bin/sh` is bash in
POSIX mode, so process substitution — `<(…)` — is **not** available in this form; pass an explicit
shell for that: `sandme /bin/bash -c 'cat <(echo hi)'`.

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
| `shared_paths` | `SANDME_SHARED_PATHS` | array of strings (env: comma-separated) | `["~/"]` | Paths the sandboxed command may **read and write**. `~/` expands to your home directory; symlinks are resolved. |
| `proxy_port` | `SANDME_PROXY_PORT` | integer | `8787` | Loopback port the egress proxy listens on. `0` picks a free port — use it when running several `sandme` invocations at once. |
| `gui_mode` | `SANDME_GUI_MODE` | boolean (env: `1` or `true`) | `false` | Also grants read+write to `~/Library`, `/private/tmp` and `/private/var/folders`. GUI apps and most editors need this for their state, caches and scratch space. |
| `allow_private_egress` | `SANDME_ALLOW_PRIVATE_EGRESS` | boolean (env: `1` or `true`) | `false` | Lets the proxy relay to your own machine and network — loopback, RFC1918, link-local. Needed for a locally hosted service (a local model server, a dev API); see [Network](#what-the-sandbox-allows) for what it re-opens. |

No config file is required — without one you get the defaults. A malformed file is reported
rather than ignored.

> **The default shares your whole home directory.** `shared_paths` defaults to `["~/"]`, so out of
> the box the sandboxed command can read and write everything under `~`, including `~/.ssh` and
> `~/.aws`. If you are sandboxing something you do not fully trust, narrow it to the project you
> are working on. See [Known limitations](#known-limitations).

### A starting config

```toml
# ~/.sandme/config.toml
shared_paths = [
  "~/Workspace",      # your projects — narrow this to taste
  "/opt/homebrew",    # Homebrew toolchain (Apple silicon; use /usr/local on Intel)
]
proxy_port = 8787
gui_mode = true       # editors need ~/Library and scratch space
```

There is a fuller, commented version in [`examples/config.toml`](examples/config.toml).

## What the sandbox allows

**Filesystem.** Read-only access to the system runtime — `/usr`, `/bin`, `/sbin`, `/System`,
`/Library`, `/Applications`, `/private/etc`. Read+write to everything in `shared_paths`, plus —
only with `gui_mode` — `~/Library` and the temp directories. Everything else is denied for both
reading and writing.

Three directories are denied outright, and no setting grants them back:

| Denied always | Why |
| --- | --- |
| `~/Library/LaunchAgents`, `~/Library/LaunchDaemons` | A plist written here is run by launchd at your next login — **outside** the sandbox. A writable one turns any share into a persistence escape. |
| `~/Library/Keychains` | Your login keychain. Apps that store credentials through Keychain Services are unaffected: `securityd` reads the files, not the sandboxed process. |

The rules are emitted last in the profile, after every grant, because Seatbelt resolves a path
against the last rule that matches it. `shared_paths = ["~/"]` does not lift them.

**Network.** TCP to the proxy port, and nothing else. Direct HTTP, DNS, raw sockets, ICMP and
listening sockets are all denied. `sandme` sets `HTTP_PROXY`, `HTTPS_PROXY` and their lowercase
forms in the command's environment, so ordinary HTTP clients use the proxy without you configuring
anything. HTTPS works through `CONNECT`.

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

**Other processes, and the kernel.** The command sees process information for its own sandbox —
itself and anything it starts — and nothing about the processes running outside it. It reads a
fixed allowlist of sysctls: the `hw.`, `machdep.cpu.`, `net.` and `sysctl.` subtrees, and the
handful of `kern.*` names ordinary programs ask for (OS version, hostname, boot time, `argmax`).
`sysctl -a` shows 798 values inside the sandbox where it shows 1844 outside.

Both used to be unrestricted, and between them they answered `KERN_PROCARGS2` for any process of
your own — which returns that process's **whole environment**. A sandboxed agent could read every
API key and token you had passed to anything else you were running, and send it out through the
proxy's permitted egress. `ps -E` was blocked, which made the profile look tighter than it was;
the kernel interface behind it was open. Fixed in
[#31](https://github.com/leopepe/sandme/issues/31): the two grants are narrow by name, and the
process-information denial is stated explicitly because `(deny default)` does not reach this one.

If a tool you sandbox needs a sysctl the allowlist does not name, it gets `EPERM`. The name is in
the kernel log — `log show --last 2m --predicate 'eventMessage CONTAINS "deny(1) sysctl-read"'` —
and that name is what a bug report needs.

## Recipes

Each of these is verified against the current build on macOS 26 (Apple silicon).

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

Put `/opt/homebrew` in `shared_paths` once and forget about it. On Intel Macs Homebrew uses
`/usr/local`, which is already readable via `/usr`. If you use Nix, add `/nix` too.

Be aware this grants **write** access to your Homebrew prefix — `shared_paths` has no read-only
mode yet ([#10](https://github.com/leopepe/sandme/issues/10)).

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

The default port `8787` collides. Use an ephemeral port:

```shell
SANDME_PROXY_PORT=0 sandme cargo test
```

## Known limitations

Read these before trusting the sandbox with something hostile.

| | Issue |
| --- | --- |
| `shared_paths` defaults to your whole home directory, so out of the box the command reads and writes everything under `~` — `~/.ssh` and `~/.aws` included. | [#12](https://github.com/leopepe/sandme/issues/12) |
| `gui_mode` widens the sandbox globally rather than per-app: it shares all of `/private/tmp` and `/private/var/folders`, not just the launched app's own container. | [#12](https://github.com/leopepe/sandme/issues/12) |
| `mach-lookup` is allowed with no service allowlist, so `osascript` can talk to running apps. No escape has been demonstrated through it, but the channel is not closed. | [#12](https://github.com/leopepe/sandme/issues/12) |
| An app-bundle CLI wrapper is only redirected when it is the first word of the command; inside a compound shell string (`sandme 'cd ~/proj && zed .'`) it is left to the shell and fails. | [#13](https://github.com/leopepe/sandme/issues/13) |
| Redirecting to the bundle executable loses the wrapper's own flags (`zed --wait`, `--version`) — the two binaries have different CLIs. Paths are unaffected. | [#13](https://github.com/leopepe/sandme/issues/13) |
| Process substitution needs an explicit shell — `/bin/sh` is bash in POSIX mode and has `<(…)` disabled — so use `sandme /bin/bash -c '…'`. And `diff <(a) <(b)` additionally needs `gui_mode`, because macOS `diff` copies non-seekable input to a temp file. | [#12](https://github.com/leopepe/sandme/issues/12) |
| The proxy runs unsandboxed. It now refuses loopback, RFC1918 and link-local destinations and requires a per-run credential, but there is no destination allowlist, and `allow_private_egress` re-opens all of it at once. | [#15](https://github.com/leopepe/sandme/issues/15) |
| `shared_paths` grants read **and** write; there is no read-only share for toolchains. | [#10](https://github.com/leopepe/sandme/issues/10) |

## Development

```shell
cargo build            # build
cargo test             # unit + integration tests
cargo clippy --all-targets
cargo fmt
```

`AGENTS.md` describes the working agreement; `docs/specs/` holds the requirements, `docs/adrs/`
the structural decisions, and `docs/guidelines/` the rules that apply across changes.
