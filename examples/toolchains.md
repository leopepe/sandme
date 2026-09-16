# Toolchains and build tools

The base read-only set covers the system runtime — `/usr`, `/bin`, `/sbin` and friends. Anything
installed anywhere else has to be shared, or its binaries cannot load their own libraries.

Verified against the current build on macOS 26 (Apple silicon). The prefixes below are macOS's;
the shape is the same on Linux.

## Homebrew

`/opt/homebrew` (Apple silicon) is not in the base read-only set, so a brew-installed binary
cannot load its dylibs:

```shell
sandme 'fish -c "echo hi"'
# dyld: Library not loaded: /opt/homebrew/opt/pcre2/lib/libpcre2-8.0.dylib
#   Reason: ... (file system sandbox blocked open())

SANDME_SHARED_PATHS="$HOME,/opt/homebrew" sandme 'fish -c "echo hi"'
# hi
```

Put the prefix in `read_only_paths` once and forget it. A toolchain has to be readable and
runnable, not writable, and that is exactly what `read_only_paths` grants:

```toml
# ~/.sandme/config.toml
read_only_paths = ["/opt/homebrew"]
```

`shared_paths` works too, at the cost of granting **write** access to the prefix — which means a
sandboxed command can rewrite the binaries your *next* run executes.

On Intel Macs Homebrew uses `/usr/local`, already readable via `/usr`.

## Nix

```toml
read_only_paths = ["/nix"]
```

## Language runtimes under your home

A version manager — rustup, nvm, mise, pyenv, rbenv — puts its toolchains under `~`, so a share
narrowed to one project has to name the manager's directory as well:

```shell
# Rust via rustup
sandme cargo test

# a narrowed share still needs the toolchain
SANDME_SHARED_PATHS="~/Workspace/my-project,~/.cargo,~/.rustup" \
  sandme cargo test
```

The read-only form is better when the build does not need to write there — but note that cargo
writes to `~/.cargo/registry`, and most version managers write caches, so a toolchain under `~`
usually does belong in `shared_paths`, not `read_only_paths`.

## Builds that reach the network

Dependency resolution goes through the proxy like any other egress, with no configuration:

```shell
sandme cargo build
sandme 'npm ci 2>&1 | tee install.log'
```

A build that needs *no* network at all is worth pinning down explicitly:

```shell
SANDME_PROXY=false sandme cargo build --offline
```

## git

Two things do not work, both by design:

- **SSH remotes** — port 22 is denied; egress goes only through the proxy.
- **HTTPS remotes** — these fall through to a username prompt with no terminal to answer it.

Local git is unaffected: `sandme git log`, `sandme git diff`, `sandme git commit` all work. On
macOS `/usr/bin/git` is an `xcrun` shim that caches into `$TMPDIR`, so it needs `gui_mode`:

```shell
SANDME_GUI_MODE=1 sandme git commit -m "fix: narrow the default share"
```

Without it git fails with `Operation not permitted`, and the message names `xcrun_db` rather than
sandme ([#29](https://github.com/leopepe/sandme/issues/29)).

## Several runs at once

`proxy_port` defaults to `0`, so each invocation binds its own OS-chosen ephemeral port and
concurrent runs never collide:

```shell
sandme cargo test &
sandme cargo clippy &
```

Pin a fixed port only when one run needs a predictable one — only one run may hold a given port at
a time:

```shell
SANDME_PROXY_PORT=9000 sandme cargo test
```
