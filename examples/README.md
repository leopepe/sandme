# examples

Worked invocations, each verified against the current build on macOS 26 (Apple silicon).
The full explanation of every flag and key is in the [project README](../README.md).

## Files

| File | What it is |
| --- | --- |
| [`config.toml`](config.toml) | A commented `~/.sandme/config.toml` to copy and edit |

## Simple commands

The defaults are enough — no config file needed.

```shell
sandme ls -la ~/Workspace                    # multiple operands: direct exec, no shell
sandme 'ls -la ~/Workspace | grep rust'      # single quoted operand: routed through /bin/sh -c
sandme -- curl -sS https://example.com/      # -- stops sandme reading the command's own flags
sandme 'cargo build 2>&1 | tee build.log'
```

Egress goes through the proxy automatically; the sandbox denies every other network destination.

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

Zed — like any IDE shipped as a `.app` with a CLI wrapper — needs its **absolute path** in the
multi-operand form, plus `gui_mode`:

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme /usr/local/bin/zed ~/Workspace/my-project
```

Ctrl-C in the launching terminal shuts Zed and the proxy down together.

`sandme 'zed ~/Workspace/'` fails with `error: cannot start app bundle` — see
[#13](https://github.com/leopepe/sandme/issues/13). Find the real path with `which zed`.

## Coding agents

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="~/Workspace/my-project,/opt/homebrew" \
  sandme claude
```

## Running several at once

The default proxy port collides across concurrent invocations. Take an ephemeral one:

```shell
SANDME_PROXY_PORT=0 sandme cargo test
```
