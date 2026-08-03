# POSIX CLI conventions

How `sandme` presents itself to the shell, so that it composes with every other tool a user
already has.

Audience: humans and code agents. Rules use MUST / MUST NOT / SHOULD.

## 1. Principle

The command line is an interface contract, and the shell is the other party. A tool that invents
its own argument grammar, writes diagnostics to stdout, or swallows exit codes cannot be piped,
scripted, or wrapped — however clean its internals are.

Baseline: POSIX.1 *Utility Conventions* (XBD §12.2). Where a widely-adopted GNU extension is
listed below, it is marked as such — it is permitted, but it is an extension, not the baseline.

`sandme` is an **exec wrapper** in the family of `env`, `nice`, `timeout` and `sudo`: it takes
options of its own, then a command to run. That family has settled conventions, and §5 applies
them.

## 2. Argument grammar

- Options MUST be single alphanumeric characters preceded by `-` (`-p`). Multi-character
  long options preceded by `--` (`--proxy-port`) are a GNU extension and SHOULD be provided as
  the readable form of every short option.
- Options without arguments MUST be groupable behind one `-`: `-ab` means `-a -b`.
- An option-argument MUST be accepted as a separate argument (`-p 8787`). Attached forms
  (`-p8787`, `--proxy-port=8787`) SHOULD also be accepted.
- Option-arguments MUST NOT be optional. An option either always takes an argument or never
  does; `-p` meaning one thing alone and another with a value is a Blocker.
- A list-valued option-argument MUST be one argument with comma-separated values
  (`--shared-paths ~/src,~/tmp`), not a repeated flag inventing its own accumulation rule.
- All options MUST precede operands. The first non-option argument ends option parsing.
- `--` MUST terminate option parsing. Everything after it is an operand, even if it starts
  with `-`.
- The relative order of options MUST NOT matter, except for mutually exclusive ones, where the
  last occurrence wins.
- Where a utility reads or writes a file operand, `-` MUST mean stdin or stdout.
- Utility and option names MUST be lowercase.

`--help` and `--version` are GNU extensions. Provide both. `--help` exits `0` and writes to
stdout when requested explicitly; a usage error writes to stderr and exits non-zero.

## 3. Streams

| Stream | Carries | Never carries |
| --- | --- | --- |
| stdout | The output the user asked for — the sandboxed command's stdout | Progress, status, warnings |
| stderr | Diagnostics, warnings, errors, progress | Data another program would parse |

- Diagnostics MUST go to stderr, prefixed with the utility name: `sandme: config error: …`.
- On success a utility SHOULD be silent. Announcing what it is about to do is chatter; it
  corrupts pipelines and hides the wrapped command's own output.
- Colour and other terminal decoration MUST be suppressed when the stream is not a TTY, and
  when `NO_COLOR` is set in the environment.
- For an exec wrapper, the child's stdout and stderr MUST pass through unmodified. Do not
  prefix, buffer, interleave or re-encode them.

## 4. Exit status

`0` is success and only success. Every non-zero value MUST mean one thing, and that meaning MUST
appear in the spec's **Exit codes / errors** table.

The exec-wrapper convention, shared with `env(1)`, `timeout(1)` and the shell:

| Status | Meaning |
| --- | --- |
| `0` | The wrapped command succeeded |
| `1`–`125` | The wrapped command's own exit status, or a `sandme` failure |
| `126` | The command was found but could not be executed |
| `127` | The command was not found |
| `128+n` | The command was terminated by signal `n` |

- The wrapped command's exit status MUST be propagated unchanged.
- Termination by signal MUST become `128+n`, not a clamped or collapsed value. A child killed by
  `SIGINT` (2) exits `130`; reporting `1` tells the caller the wrong thing.
- `sandme`'s own failures MUST NOT collide with statuses the wrapped command can plausibly
  return where the spec can avoid it; reserve and document them.

## 5. Exec-wrapper shape

The command and its arguments are **operands**, not a single quoted string.

```
sandme [options] [--] <command> [args...]
```

- `sandme zed ~/Workspace` MUST work without quoting. Requiring `sandme 'zed ~/Workspace'`
  pushes word-splitting onto the user and breaks paths containing spaces.
- The child's own options MUST reach the child: `sandme -- curl -sS https://example.com` passes
  `-sS` to `curl`. Without `--`, the first operand still ends `sandme`'s option parsing.
- The child MUST NOT be re-parsed, re-quoted, or passed through a shell. Build the argument
  vector directly.

In `clap`, that shape is a trailing var-arg operand that keeps hyphenated values:

```rust
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Cli {
    /// Command to run sandboxed, with its arguments
    #[arg(required = true, trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}
```

## 6. Environment

- Environment variables MUST be uppercase and prefixed `SANDME_`.
- Precedence is fixed: command-line options override environment variables, which override the
  config file, which overrides built-in defaults. This ordering MUST NOT vary per setting.
- Standard variables MUST be respected where they apply: `NO_COLOR`, `HOME`, `TMPDIR`.
- Variables injected into the child (`HTTP_PROXY`, `HTTPS_PROXY`) MUST NOT overwrite a value the
  user set, unless the spec says they must — say which, and why.

## 7. Review checklist

- [ ] Options precede operands; `--` terminates option parsing.
- [ ] Every short option has a long form; no option-argument is optional.
- [ ] Diagnostics on stderr with the `sandme: ` prefix; stdout carries only the child's output.
- [ ] Nothing is printed on the success path that the user did not ask for.
- [ ] Colour suppressed when not a TTY or when `NO_COLOR` is set.
- [ ] Child exit status propagated; signal deaths reported as `128+n`.
- [ ] Every non-zero status documented in the spec's Exit codes table.
- [ ] The child receives its own arguments unshelled and unquoted.
- [ ] Config precedence matches §6 for every setting.

## 8. Related

- `docs/guidelines/code/simplicity.md` — the code behind this surface.
- `docs/guidelines/code/rust.md` — error messages are the text this surface prints.
- `docs/guidelines/tests/testing.md` — CLI behaviour is proven by integration tests.
- `docs/guidelines/sdd/spec-driven-development.md` — the Interface contract section that binds
  flags, defaults and exit codes.
