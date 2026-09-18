# Tasks

- [x] 1. Drop `Deserialize` and the per-field `#[serde(default)]` from `Config`; add `Layer`,
      `Layer::from_environment` and `Config::merge`
      depends-on: none
      touches: src/config.rs
- [x] 2. Parse the config file once into a `Layer`; merge defaults, file and environment in
      `load_from`; delete `apply_env`
      depends-on: 1
      touches: src/config.rs
- [x] 3. Take `widening_warnings` off `FileKeys`/`EnvKeys`/`Provenance` and onto the environment
      `Layer`; delete all four
      depends-on: 2
      touches: src/config.rs
- [x] 4. Move the unit tests onto the merge shape; the three `provenance()` tests go with the
      function, the FR-1001 – FR-1004 behaviour tests stay untouched as the contract
      depends-on: 3
      touches: src/config.rs
- [x] 5. Restore FR-1004 traceability: its Verification row named the three `provenance_*` tests
      deleted in task 4, so add `attributes_a_setting_to_the_environment_even_when_the_config_file_set_it_too`
      and repoint the row; correct T-1001 and the NFR-1002 note, which described the deleted
      mechanism; add a changelog row recording that no requirement changed
      depends-on: 4
      touches: src/config.rs, docs/specs/0010-warn-env-sourced-widening.md
- [x] 6. Quality gate: `cargo fmt`, `cargo clippy --all-targets`, `cargo build`, `cargo test`,
      `cargo doc --no-deps` — all clean from a `cargo clean`, 73 unit + 62 integration tests pass
      depends-on: 5
      touches: none
