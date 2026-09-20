# Template Method for SPEC-0013 routing (R1)

Keep resolve as trait default with two overridable identity steps. Seatbelt overrides only redirect_program and redirect_shell_command.

## Archived

Landed in commit `080cd0a` (PR #70, issue #60) — finding R1 of the refactoring review of
2026-09-16. Archived 2026-09-20; both tasks verified against the code before archiving:
the trait default `resolve` sits at `src/sandbox/mod.rs:55` with the `redirect_program` /
`redirect_shell_command` hooks, and `Seatbelt::resolve` is gone, overriding only those two
(`src/sandbox/seatbelt/mod.rs:33,37`).

Checked against: SPEC-0013 (single-operand shell routing) — operand routing behaviour
unchanged, the refactor only moves where the branch lives. SPEC-0015 (sandbox backend
abstraction) — the change is an instance of that abstraction, not a departure from it.
No ADR in `docs/adrs/` bears on it. No spec delta: `--skip-specs`, since requirements live
in `docs/specs/` and none changed.
