# examples

Worked invocations, verified against the current build on macOS 26 (Apple silicon) unless a page
says otherwise. The command forms are the same on Linux; the paths here — `/opt/homebrew`, the
`.app` bundles — are macOS's.

Every flag and config key used below is explained in
[`docs/reference/configuration.md`](../docs/reference/configuration.md); what the sandbox actually
permits is in [`docs/reference/sandbox-model.md`](../docs/reference/sandbox-model.md).

| | What it covers |
| --- | --- |
| [`agents.md`](agents.md) | Coding agents — Claude Code, Codex, Pi, Cursor CLI, local models, no-network runs |
| [`editors.md`](editors.md) | Editors and IDEs — Neovim, Zed, Cursor, VS Code, and the `.app` wrapper redirect |
| [`toolchains.md`](toolchains.md) | Homebrew, Nix, language runtimes, builds, git, parallel runs |
| [`config.toml`](config.toml) | A commented `~/.sandme/config.toml` to copy and edit |

## The two command forms

```shell
sandme ls -la ~/Workspace                    # multiple operands: direct exec, no shell
sandme 'ls -la ~/Workspace | grep rust'      # single quoted operand: routed through /bin/bash -c
sandme -- curl -sS https://example.com/      # -- stops sandme reading the command's own flags
sandme 'cargo build 2>&1 | tee build.log'
```

Use the quoted form when you want shell features — pipes, redirection, globbing, `&&`, variable
expansion, process substitution. Use the multi-operand form for everything else; it involves no
shell at all.

## Simple commands

The defaults are enough — no config file needed. `shared_paths` defaults to the directory you run
in, so the command reads and writes that directory and nothing else under `~`:

```shell
sandme ls -la
sandme cargo test
sandme rg TODO src/
```

Egress goes through the proxy automatically; the sandbox denies every other destination — all of
them on macOS, every other outbound TCP one on Linux.

## Where to start

Running an agent for the first time? [`agents.md`](agents.md). Something failed on startup and you
do not know why? It is nearly always a missing toolchain prefix, a state directory outside the
share, or `gui_mode` left off — [`toolchains.md`](toolchains.md) covers the first,
[`agents.md`](agents.md) the other two.
