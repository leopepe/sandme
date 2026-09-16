# What the sandbox allows

Everything is denied by default. What follows is everything the sandboxed command gets back, and
why. The settings named here are documented in [Configuration](configuration.md).

## Filesystem — macOS

Read-only access to the system runtime — `/usr`, `/bin`, `/sbin`, `/System`, `/Library`,
`/Applications`, `/private/etc`. Read+write to everything in `shared_paths`, plus — only with
`gui_mode` — `~/Library` and your per-user temp directory (`$TMPDIR`). `gui_mode` does **not** open
the world-shared `/private/tmp` or all of `/private/var/folders`, only the temp directory macOS
gives your own session. Everything else is denied for both reading and writing.

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

## Filesystem — Linux

Read+execute on the system runtime — `/usr`, `/bin`, `/sbin`, `/lib`, `/lib64`, `/opt` — and read
on `/etc` and the random devices. Read+write to everything in `shared_paths`, to `/dev/null` and
`/dev/tty`, and — only with `gui_mode` — to `$XDG_CONFIG_HOME`, `$XDG_CACHE_HOME` and
`$XDG_DATA_HOME` (falling back to `~/.config`, `~/.cache` and `~/.local/share`) and to
`$XDG_RUNTIME_DIR` (falling back to `/tmp`). Everything else is denied.

`/proc` and `/sys` are granted to nothing. Landlock is allow-only with no deny primitive, so the
one way to keep another process's `/proc/<pid>/environ` out of reach is never to grant `/proc` at
all. For the same reason a `shared_paths` entry that would re-admit either tree — `/` itself, or an
ancestor of one of them — is refused rather than honoured.

## Pseudo-terminals

A command may allocate one, so an editor's integrated terminal and its login-shell environment
loading work ([#29](https://github.com/leopepe/sandme/issues/29)). This is a real capability — a
PTY is a kernel object the command creates — and needs no `gui_mode`. On Linux that is `/dev/ptmx`
and the `/dev/pts` subtree, granted read+write for the same reason.

## Network

TCP to the proxy port, and nothing else. On macOS direct HTTP, DNS, raw sockets, ICMP and
listening sockets are all denied. On Linux the rule is a port number with no address — Landlock
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

## What it does not cover

[Known limitations](limitations.md) lists the gaps that are real and open. Read them before
trusting the sandbox with something hostile.
