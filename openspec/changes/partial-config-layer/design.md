# Design: one partial layer, produced twice

`Config` stops deriving `Deserialize`: every default now has exactly one definition, in
`impl Default`, instead of one there and one in a `#[serde(default = "…")]` per field — the
disagreement that comment at the head of `config.rs` records as a past bug.

`Layer` carries the same six settings as `Option`s, where absent means "this source said nothing".
It is produced twice: `toml::from_str` for the config file (one parse, so the `toml::Table` pass
that existed only for `contains_key` goes), and `Layer::from_environment` for the `SANDME_*`
variables. `Config::default().merge(&file).merge(&env)` is FR-009's precedence, and the baseline
FR-1003 judges a broadened share against is read between the two merges.

Provenance needs no type: the environment layer is applied last, so a key it carries is the key
that took effect. `provenance(file.gui_mode, env.gui_mode) == Provenance::Environment` becomes
`env.gui_mode == Some(true)` — which is also why the file layer never reaches `widening_warnings`,
matching the old code, where `provenance(…) == Environment` ignored its file argument.
