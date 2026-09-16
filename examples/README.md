# examples

Worked invocations, each verified against the current build on macOS 26 (Apple silicon). The
command forms are the same on Linux; the paths below — `/opt/homebrew`, the `.app` bundles — are
macOS's. The full explanation of every flag and key is in the [project README](../README.md).

## Files

| File | What it is |
| --- | --- |
| [`config.toml`](config.toml) | A commented `~/.sandme/config.toml` to copy and edit |

## Simple commands

The defaults are enough — no config file needed.

```shell
sandme ls -la ~/Workspace                    # multiple operands: direct exec, no shell
sandme 'ls -la ~/Workspace | grep rust'      # single quoted operand: routed through /bin/bash -c
sandme -- curl -sS https://example.com/      # -- stops sandme reading the command's own flags
sandme 'cargo build 2>&1 | tee build.log'
```

Egress goes through the proxy automatically; the sandbox denies every other destination — all of
them on macOS, every other outbound TCP one on Linux.

## Homebrew-installed tools

`/opt/homebrew` is not in the base read-only set, so brew binaries cannot load their dylibs:

```shell
sandme 'fish -c "echo hi"'
# dyld: Library not loaded: /opt/homebrew/opt/pcre2/lib/libpcre2-8.0.dylib
#   Reason: ... (file system sandbox blocked open())

SANDME_SHARED_PATHS="$HOME,/opt/homebrew" sandme 'fish -c "echo hi"'
# hi
```

## Neovim

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme nvim ~/Workspace/my-project
```

Confined to one project instead of all of `~`:

```shell
SANDME_GUI_MODE=1 \
SANDME_SHARED_PATHS="~/Workspace/my-project,/opt/homebrew,~/.config/nvim,~/.local/share/nvim,~/.local/state/nvim,~/.cache/nvim" \
  sandme nvim ~/Workspace/my-project
```

System `vim` from `/usr/bin` needs neither: `sandme vim <file>` works with the defaults.

## Zed

Zed — like any IDE shipped as a `.app` with a CLI wrapper — needs `gui_mode`; the command name is
enough, because `sandme` notices the wrapper points into an `.app` and runs the bundle's own
executable instead:

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme zed ~/Workspace/my-project
```

Ctrl-C in the launching terminal shuts Zed and the proxy down together.

The redirect only applies to the first word, so `sandme 'cd ~/proj && zed .'` is left to the shell
and fails — see [#13](https://github.com/leopepe/sandme/issues/13). Name the app first.

## Coding agents

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme claude
```

Narrowing the share to the project alone fails with `error: An internal error occurred (EPERM)`:
`claude` reads and rewrites state next to `~/.claude.json` — see the project README.

## Running several at once

`proxy_port` defaults to `0`, so each invocation binds its own OS-chosen ephemeral port and
concurrent runs never collide:

```shell
sandme cargo test &
sandme cargo clippy &
```

Pin a fixed port only when one run needs a predictable one:

```shell
SANDME_PROXY_PORT=9000 sandme cargo test
```
