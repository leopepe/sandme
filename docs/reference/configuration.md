# Configuration

`sandme` reads `~/.sandme/config.toml`. Every key has a matching environment variable, and the
environment always wins.

No config file is required — without one you get the defaults. A malformed file is reported
rather than ignored.

## Keys

| Key | Environment variable | Type | Default | What it does |
| --- | --- | --- | --- | --- |
| `shared_paths` | `SANDME_SHARED_PATHS` | array of strings (env: comma-separated) | the current working directory | Paths the sandboxed command may **read and write**. `~/` expands to your home directory; symlinks are resolved. Set `["~/"]` to share your whole home. |
| `read_only_paths` | `SANDME_READ_ONLY_PATHS` | array of strings (env: comma-separated) | empty | Paths the sandboxed command may **read and execute** but **not write** — a toolchain prefix (a Homebrew directory, a language runtime) whose binaries must run while writes stay confined to `shared_paths`. `~/` expands and symlinks are resolved, as for `shared_paths`. |
| `proxy` | `SANDME_PROXY` | boolean (env: `1` or `true`) | `true` | Whether to start the egress proxy and route the command through it. Set `false` to run with **no egress on macOS, and no outbound TCP on Linux** (see below). |
| `proxy_port` | `SANDME_PROXY_PORT` | integer | `0` | Loopback port the egress proxy listens on. `0` (the default) lets the OS pick a free ephemeral port, so parallel `sandme` runs never collide. Set a non-zero value to pin a fixed, predictable port. |
| `gui_mode` | `SANDME_GUI_MODE` | boolean (env: `1` or `true`) | `false` | Also grants read+write to the per-user state and scratch directories: `~/Library` and `$TMPDIR` on macOS, the XDG config, cache, data and runtime directories on Linux. GUI apps and most editors need this for their state, caches and scratch space. |
| `allow_private_egress` | `SANDME_ALLOW_PRIVATE_EGRESS` | boolean (env: `1` or `true`) | `false` | Lets the proxy relay to your own machine and network — loopback, RFC1918, link-local. Needed for a locally hosted service (a local model server, a dev API); see [the sandbox model](sandbox-model.md#network) for what it re-opens. |

> **The default shares the directory you run `sandme` in.** `shared_paths` defaults to the current
> working directory, so out of the box the sandboxed command reads and writes that directory and
> nothing else under `~` — `~/.ssh` and `~/.aws` are not exposed. To share more, name it in
> `shared_paths`; `["~/"]` restores the old whole-home behaviour.

> **`proxy = false` disables all network remotes**, not just the HTTP proxy: with no proxy running
> the command gets no `HTTP(S)_PROXY` and no git-over-SSH tunnel, so both HTTP(S) and git-over-SSH
> remotes fail. That is the point — a no-network run — and an SSH git failure under `proxy = false`
> is expected, not a bug. On macOS the sandbox then denies all outbound network; on Linux it denies
> all outbound TCP, which is as far as Landlock reaches.

## A starting config

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

There is a fuller, commented version in [`examples/config.toml`](../../examples/config.toml), and
worked invocations for editors, agents and toolchains in
[`examples/README.md`](../../examples/README.md).
