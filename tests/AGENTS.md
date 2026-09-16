# AGENTS.md — tests/

Integration tests are not an extension of the unit tests. A test here emulates a user
interacting with the application.

## Rules

- An integration test exercises the application's interfaces with the user in full — for the CLI,
  every main feature and every option.
- Drive the application the way a user drives it. Do not reach past the interface.
- Challenge the feature's usability from the user's perspective and interaction patterns.
