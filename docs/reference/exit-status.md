# Exit status

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
