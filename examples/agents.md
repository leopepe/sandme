# Coding agents

An agent is the case `sandme` exists for: it reads and writes your files and it talks to a remote
model, and you want the first bounded and the second observable.

Every agent below needs the same three things:

- **`gui_mode`** — agents write caches, logs and scratch files under `~/Library` (macOS) or the
  XDG directories (Linux). Without it they start and then fail on their first write.
- **a share that covers their own state** — most keep credentials and session history under `~`,
  outside your project.
- **the proxy left on** (the default) — it is the only route out, and the only place the traffic
  is visible.

**Verified against the current build on macOS 26 (Apple silicon): `claude`, `codex`.** The others
follow the same shape; their state paths come from each tool's own documentation rather than from
a run here, so check them against your install before pinning a narrow share.

## Claude Code

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme claude
```

Narrowing the share to the project alone fails:

```shell
# error: An internal error occurred (EPERM)
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="~/Workspace/my-project,/opt/homebrew" \
  sandme claude
```

`claude` is a native binary that finds its own home through the passwd database rather than
`$HOME`, and it reads and rewrites state next to `~/.claude.json`. Sharing `~/.claude` and
`~/.claude.json` explicitly is not enough. Until `shared_paths` can express that shape
([#10](https://github.com/leopepe/sandme/issues/10)), share `$HOME` and rely on the network
confinement rather than the filesystem confinement.

## OpenAI Codex CLI

Interactive:

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme codex
```

Non-interactive — one prompt, no TUI, which is the form worth wrapping in a script:

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme codex exec "add a regression test for the parser"
```

Codex keeps its credentials and history in `~/.codex`, so a share narrowed to the project must
name that directory too:

```shell
SANDME_GUI_MODE=1 \
SANDME_SHARED_PATHS="~/Workspace/my-project,~/.codex,/opt/homebrew" \
  sandme codex exec "run the tests and fix what fails"
```

## Pi

[Pi](https://pi.dev) installs as `pi` and keeps settings, credentials and sessions in
`~/.pi/agent`. Interactive:

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme pi
```

Print mode — one prompt, final answer to stdout, then exit:

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme pi -p "summarise what this crate does"
```

Narrowed to one project plus Pi's own state:

```shell
SANDME_GUI_MODE=1 \
SANDME_SHARED_PATHS="~/Workspace/my-project,~/.pi,/opt/homebrew" \
  sandme pi -p "explain the error handling in src/"
```

Pi's RPC mode (`pi --mode rpc`) speaks JSONL over stdin/stdout. `sandme` passes both through
untouched, so an editor extension or CI driver on the outside talks to a sandboxed agent on the
inside with nothing else to configure:

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme pi --mode rpc
```

## Cursor CLI

Cursor's terminal agent installs as `agent` — into `~/.local/bin` by default, which is not in the
base read-only set, so share wherever the installer put it.

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme agent
```

Print mode. `--force` is what makes it apply edits rather than only propose them — worth pairing
with a share narrowed to the one project you mean it to touch:

```shell
SANDME_GUI_MODE=1 \
SANDME_SHARED_PATHS="~/Workspace/my-project,~/.cursor,~/.local/bin" \
  sandme agent -p --force "refactor the config loader to use serde defaults"
```

`CURSOR_API_KEY` in your environment is inherited by the sandboxed process like any other
variable; `sandme` does not read or rewrite it.

## Any other agent

The pattern does not depend on the tool:

```shell
SANDME_GUI_MODE=1 SANDME_SHARED_PATHS="$HOME,/opt/homebrew" \
  sandme <agent-command>
```

If it fails on startup, it is almost always one of three things — a toolchain prefix that is not
shared (`/opt/homebrew`, `/nix`, a runtime under `~`), a state directory outside the share, or
`gui_mode` left off. Widen to `$HOME` first to confirm it runs at all, then narrow back down.

## Agents against a local model

An agent pointed at Ollama or any locally hosted API needs the proxy's private-destination refusal
lifted, because `127.0.0.1` is exactly what it blocks:

```shell
SANDME_ALLOW_PRIVATE_EGRESS=1 SANDME_GUI_MODE=1 sandme claude
```

That re-opens your whole machine and LAN to the sandboxed command. See
[the sandbox model](../docs/reference/sandbox-model.md#network) for what that means.

## A no-network run

To let an agent read and edit files with no way out at all — useful for a review pass, or for
anything you do not want phoning home:

```shell
SANDME_PROXY=false sandme <agent-command>
```

The command gets no `HTTP(S)_PROXY` and no git-over-SSH tunnel. An agent that needs a model API
will fail; that is the point.

## Two warnings you will see

Passing `SANDME_GUI_MODE` or `SANDME_SHARED_PATHS` on the command line prints a warning: a
sandboxed command could plant those in a shell rc to pre-widen your *next* run
([#30](https://github.com/leopepe/sandme/issues/30)). The environment form is fine for trying
something out; put the settings you keep in `~/.sandme/config.toml` instead, and the warning goes
away:

```toml
# ~/.sandme/config.toml
shared_paths = ["~/"]
read_only_paths = ["/opt/homebrew"]
gui_mode = true
```

Then every agent above is just `sandme claude`, `sandme codex`, `sandme pi`.
