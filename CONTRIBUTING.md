# Contributing to sandme

`sandme` is a sandbox. A change that looks cosmetic can widen it, so this repository asks for a
little more process than its size suggests. This file routes you to that process; it does not
repeat it.

`AGENTS.md` is the working agreement, and `docs/guidelines/` holds the rules that apply across
changes. Each guideline is authoritative for its domain. Read the one that covers what you are
touching rather than working from memory.

## Build, test, lint

```shell
cargo build
cargo test                    # unit tests plus the integration tests in tests/
cargo clippy --all-targets
cargo fmt
```

Before opening a pull request the quality gate must pass: `cargo fmt`, `cargo clippy
--all-targets`, `cargo build`, `cargo test` — in that order, with no warnings and no failures.
The order is not arbitrary and the reasoning is in `docs/guidelines/code/quality-gates.md`,
which is the authority on the gate. Run the commands; do not assert the result.

`docs/guidelines/tests/testing.md` says where a test belongs and what it may assert. Unit tests
live at the foot of the module they cover, integration tests in `tests/`.

## Specs come before code

This is a spec-driven repository. A change to observable behaviour — the CLI surface, the
profile, the config keys, exit statuses — needs a spec in `docs/specs/` at `Status: Accepted`
before the implementation is written. A trivial change takes the fast path instead.

`docs/guidelines/sdd/spec-driven-development.md` defines both, and §9 is the fast-path test.
Read it before writing code for anything you cannot describe as a typo, a bump or a rename.

## Platforms

The sandbox is applied by the operating system, so it cannot be verified anywhere else.

| Change touches | Verify on |
| --- | --- |
| The Seatbelt profile, `sandbox-exec`, anything macOS-specific | macOS, on a real Mac |
| The Landlock backend, anything Linux-specific | Linux, kernel 6.7 or newer with Landlock enabled |

Kernel 6.7 is Landlock ABI v4, the first version that can restrict outbound TCP. An older kernel
builds and runs the tests that do not need the sandbox, and tells you nothing about the ones that
do.

CI covers `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`. If
you could not run a platform's tests yourself, say so in the pull request instead of leaving the
reviewer to guess.

## Commits and pull requests

Commits follow [Conventional Commits](https://www.conventionalcommits.org/): a type, an optional
scope, a colon, then an imperative summary.

```
feat(sandbox): add a Linux/Landlock backend behind the Backend trait
fix(landlock): mask dir-only rights on file paths
refactor: inject $HOME instead of reading it from the environment
ci: macOS Apple Silicon only; drop the Intel x86_64 leg
```

The types in use are `feat`, `fix`, `refactor` and `ci`; `docs`, `test` and `chore` take the same
form. Reference the requirement ID the change traces to, or the issue it closes.

One pull request per change. A commit that fails the quality gate is a broken bisect point, so
run the gate before each commit, not once at the end.

## Security-relevant changes

Some files *are* the sandbox: `src/profile.rs`, `src/proxy.rs`, `src/egress.rs` and
`src/sandbox.rs`. A change to any of them gets a narrower review before the pull request opens.
The table under **Reviewing** in `AGENTS.md` lists which review each kind of change triggers and
why. Nothing automates it — the table is the trigger.

If you have found a way *out* of the sandbox, do not open a pull request or an issue. Report it
privately first: see `SECURITY.md`.

## Licence

`sandme` is MIT licensed. A contribution submitted to this repository is contributed under the
same terms, as stated in `LICENSE`.
