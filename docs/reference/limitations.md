# Known limitations

Read these before trusting the sandbox with something hostile. Each row links the issue tracking
it.

| | Issue |
| --- | --- |
| `mach-lookup` is still allowed with no service allowlist. Cross-application AppleEvents are now denied (`appleevent-send`), but narrowing the mach-lookup family to an allowlist is deferred — no escape has been demonstrated, but the channel is not fully closed. | [#12](https://github.com/leopepe/sandme/issues/12) |
| An app-bundle CLI wrapper is only redirected when it is the first word of the command; inside a compound shell string (`sandme 'cd ~/proj && zed .'`) it is left to the shell and fails. | [#13](https://github.com/leopepe/sandme/issues/13) |
| Redirecting to the bundle executable loses the wrapper's own flags (`zed --wait`, `--version`) — the two binaries have different CLIs. Paths are unaffected. | [#13](https://github.com/leopepe/sandme/issues/13) |
| `diff <(a) <(b)` needs `gui_mode`, because macOS `diff` copies non-seekable process-substitution input to a temp file under `$TMPDIR`. (Process substitution itself works in the single-operand form since it is routed through `/bin/bash`.) | [#12](https://github.com/leopepe/sandme/issues/12) |
| The proxy runs unsandboxed. It now refuses loopback, RFC1918 and link-local destinations and requires a per-run credential, but there is no destination allowlist, and `allow_private_egress` re-opens all of it at once. | [#15](https://github.com/leopepe/sandme/issues/15) |
| macOS `/usr/bin/git` is an `xcrun` shim that caches into `$TMPDIR` under `/private/var/folders`. Without `gui_mode` that write is denied and git fails with `Operation not permitted` — the message names `xcrun_db`, not sandme. Run with `gui_mode` on, which grants the temp directories. | [#29](https://github.com/leopepe/sandme/issues/29) |
| git cannot reach a real remote: SSH remotes (port 22) are denied by design — egress goes only through the proxy — and HTTPS remotes fall through to a username prompt with no terminal to answer it. | [#29](https://github.com/leopepe/sandme/issues/29) |
