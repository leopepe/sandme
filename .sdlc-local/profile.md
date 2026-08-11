# Project Profile

## Identity
- **Name:** sandme
- **Language:** Rust 2024
- **Type:** CLI binary (macOS only) — runs a coding IDE or agent inside a Seatbelt sandbox with egress forced through a managed proxy

## Build & Quality
- **Format:** `cargo fmt --check`
- **Lint:** `cargo clippy --all-targets`
- **Build:** `cargo build`
- **Test:** `cargo test`
- **Doc:** `cargo doc --no-deps`
- **Quality gate order:** fmt → clippy → build → test → doc

`docs/guidelines/code/quality-gates.md` is the project authority on the gate.

## Local Guidelines

| Domain | Path |
|--------|------|
| architecture | `docs/guidelines/architecture/` |
| code | `docs/guidelines/code/` |
| doc-writer | `docs/guidelines/doc-writer/` |
| general | `docs/guidelines/general/` |
| performance | `docs/guidelines/performance/` |
| sdd | `docs/guidelines/sdd/` |
| tests | `docs/guidelines/tests/` |

## Language Layer
- **Language:** Rust 2024
- **pi package:** `pi-rust-skills`
- **pi skills to prefer:** `/skill:rust-code-review`, `/skill:rust-debugging`, `/skill:rust-testing`
- **Fallback:** sdlc-standards skills + cargo conventions
