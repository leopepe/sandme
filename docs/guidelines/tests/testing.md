# Testing

Where tests live, what they are allowed to assert, and what counts as covered.

Audience: humans and code agents. Rules use MUST / MUST NOT / SHOULD.

## 1. Principle

Test this repository's logic and this repository's observable behaviour. A test that fails only
when a dependency changes its internals is testing the dependency, not `sandme`.

Tests are written before the code or alongside it, never after — see
`docs/guidelines/sdd/spec-driven-development.md` §3.3.

## 2. Kinds of test

| Kind | Lives in | Covers | Shape |
| --- | --- | --- | --- |
| Unit | `#[cfg(test)] mod tests` at the foot of the module | One module's own logic, in process | Plain `#[test]` functions |
| Integration | `tests/*.rs` | The built binary, or two or more modules together | Given / When / Then |

- Every module MUST carry unit tests for its own logic.
- A test that crosses the system boundary MUST be an integration test. Invoking the CLI, parsing
  arguments, reading a config file from disk, spawning a child and driving the proxy are all
  boundary crossings.
- A test that exercises two or more modules together MUST be an integration test, even when it
  never leaves the process.

## 3. What to assert

- Assert behaviour this repository defines. MUST NOT write a test whose subject is a dependency:
  that `clap` rejects an unknown flag, that `toml` deserialises a table, that `tokio` schedules a
  task.
- Assert observable outcomes — return values, exit status, stdout, stderr, files written. An
  integration test MUST NOT reach into private state to make its assertion; if the behaviour is
  not observable from outside, it is a unit test.
- One behaviour per test. The test name states that behaviour: `propagates_child_exit_status`,
  not `test_run_1`.

## 4. External dependencies

- A costly or non-deterministic dependency MUST be mocked: the network, the clock, the filesystem
  outside a temporary directory, a spawned editor or agent.
- Mock at the boundary this repository owns — its own function or type. Do not mock a type
  belonging to a dependency.
- An integration test MAY exercise a real external dependency where a requirement calls for it.
  It MUST then be isolated so that the rest of the suite passes without that dependency
  available.

## 5. Given / When / Then

Integration tests MUST be written as Given / When / Then, one comment per phase:

```rust
#[test]
fn propagates_child_exit_status() {
    // Given a command that exits 3
    // When it is run under sandme
    // Then sandme exits 3
}
```

Name the test after the Then. Each test asserts one Then; a second Then is a second test.

## 6. Coverage

The target is behavioural coverage, not a unit-test line percentage.

- Every `FR-`/`NFR-` in an accepted spec MUST be proven by a test named in that spec's
  Verification table — see `docs/guidelines/sdd/spec-driven-development.md` §6.
- Every input that can vary MUST be tested per class of variation: valid, invalid, absent,
  boundary.
- A line percentage over untested behaviour is not coverage. Report an uncovered behaviour as a
  gap; MUST NOT close it by asserting something trivially true.
- `cargo test` MUST pass. Verify by running it, not by asserting it.

## 7. Review checklist

- [ ] Every module has unit tests for its own logic.
- [ ] Every boundary-crossing or multi-module test lives in `tests/`.
- [ ] No test's subject is a dependency's behaviour.
- [ ] Every integration test follows Given / When / Then and is named after its Then.
- [ ] Costly and non-deterministic dependencies are mocked at a boundary this repo owns.
- [ ] Every `FR-`/`NFR-` in the spec maps to a test that exists and passes.
- [ ] Every varying input is tested valid, invalid, absent and at its boundary.

## 8. Related

- `docs/guidelines/sdd/spec-driven-development.md` — Verification table and traceability chain.
- `docs/guidelines/code/quality-gates.md` — when `cargo test` must be run, and what may not be
  silenced to make it pass.
- `docs/guidelines/code/rust.md` — doc examples are tests; where errors are defined.
- `docs/guidelines/architecture/posix.md` — the exit codes and stream behaviour integration tests assert.
