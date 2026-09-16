# Editors and IDEs

An editor needs more than a CLI tool does: state and caches under `~`, scratch space, and — for
anything shipped as a macOS `.app` — a redirect past its CLI wrapper. `gui_mode` covers the first
two; `sandme` does the third by itself.

Verified against the current build on macOS 26 (Apple silicon). The paths below — `/opt/homebrew`,
the `.app` bundles — are macOS's; the command shapes carry over to Linux.

## System vim

Nothing to configure. `/usr/bin` is in the base read-only set:

```shell
sandme vim src/main.rs
```

## Neovim

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

Confined to one project instead of all of `~` — the project plus Neovim's own four directories:

```shell
SANDME_GUI_MODE=1 \
SANDME_SHARED_PATHS="~/Workspace/my-project,/opt/homebrew,~/.config/nvim,~/.local/share/nvim,~/.local/state/nvim,~/.cache/nvim" \
  sandme nvim ~/Workspace/my-project
```

## Zed

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme zed ~/Workspace/my-project
```

Zed launches, runs sandboxed, and Ctrl-C in the launching terminal shuts it and the proxy down
together. The quoted form (`sandme 'zed ~/Workspace/my-project'`) and the wrapper's absolute path
work the same way.

`zed` on `PATH` is a CLI wrapper that asks LaunchServices to open `Zed.app`, and LaunchServices is
blocked inside the sandbox. `sandme` looks the command up on `PATH`, notices it points into an
`.app`, and runs the bundle's own executable instead.

## Cursor, VS Code, and other `.app` IDEs

The same redirect applies to any IDE shipped as a macOS `.app` with a CLI wrapper on `PATH`:

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme cursor ~/Workspace/my-project

SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme code ~/Workspace/my-project
```

An IDE with a built-in agent gets the sandbox for free: the agent is a child process, and children
inherit the sandbox. Its model traffic goes through the same proxy, and its file access stops at
the same `shared_paths`.

## Two things the redirect costs you

**Wrapper-only flags stop working.** The wrapper and the bundle executable do not have the same
CLI — Zed's wrapper has `--wait`, `--new` and `--version`; its bundle executable has `--diff` and
`--user-data-dir`. Paths work on both, so opening a project is unaffected:

```shell
sandme zed --version
# error: unexpected argument '--version' found
```

`EDITOR='zed --wait'` will not block, either. Pass paths, not wrapper flags
([#13](https://github.com/leopepe/sandme/issues/13)).

**Only the first word is redirected.** A wrapper buried in a compound shell string is left to the
shell and fails:

```shell
sandme 'cd ~/proj && zed .'     # not redirected
sandme zed ~/proj               # redirected
```

That is deliberate: working out which word of a compound command is the program means guessing,
and a wrong guess would run something you did not ask for. Name the app first, or use the
multi-operand form.
