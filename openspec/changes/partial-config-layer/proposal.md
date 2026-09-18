# One partial config layer for provenance (R3)

Replace `FileKeys`, `EnvKeys`, `Provenance` and `provenance()` in `src/config.rs` with a single
`Layer` of `Option` fields, deserialised from the config file and read from the environment, then
merged onto `Config::default()`. An `Option` that is `Some` *is* the provenance bit.

**Requirements served.** No requirement changes. FR-009 (the environment overrides the config
file) and FR-1001 – FR-1004 (warn about a widening setting the environment supplied) of
SPEC-0010 keep their behaviour exactly; this is the structural half of the refactoring review of
2026-09-16 (issue #62), and qualifies as trivial under spec-driven-development.md §9 — no
observable behaviour, no interface and no stated requirement moves. The existing
`warns_when_a_widening_setting_comes_from_the_environment` and
`stays_silent_when_the_same_setting_comes_from_the_config_file` integration tests are the
contract.

**Platforms.** Both. `src/config.rs` is platform-independent; neither backend is touched.

**Non-goals.**

- No change to which settings exist, their names, their defaults or their precedence.
- No change to the warning text or to which settings warn. `proxy` still takes no provenance
  bit — `proxy = false` narrows access rather than widening it, so it stays an explicit omission
  in `widening_warnings`, not something the type enforces.
- No change to the sandbox, the proxy or egress. None of the security-critical files is touched.
