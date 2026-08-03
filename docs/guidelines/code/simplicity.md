# Simplicity

How code in this repository is kept readable and free of speculative machinery.

Audience: humans and code agents. Rules use MUST / MUST NOT / SHOULD.

## 1. Principle

**Keep it simple, stupid.** The simplest code that satisfies an accepted requirement is the
correct code. Simplicity is a constraint on the design, not a preference to be traded away for
flexibility, symmetry, or future convenience.

Two corollaries govern every rule below:

- **You are not going to need it.** Code exists to satisfy a requirement that exists today.
- **Evidence, not anticipation.** A case earns code when it is proven, not when it is imagined.

When simplicity clashes with an edge case, simplicity wins. See §4 for what makes an edge case
real.

## 2. Size limits

A unit that does not fit in your head is not simple, whatever else is true of it. These limits
replace the earlier "3 page scrolls", which was unmeasurable.

| Unit | Limit | Enforced by |
| --- | --- | --- |
| Function body | 50 lines | `clippy::too_many_lines` |
| Cognitive complexity | 15 | `clippy::cognitive_complexity` |
| Nesting inside a function | 3 levels | `clippy::excessive_nesting` |
| Function parameters | 5 | `clippy::too_many_arguments` |
| File / module | 400 lines | Review — no lint exists |

Thresholds live in `clippy.toml`; lint levels live in `[lints.clippy]` in `Cargo.toml`. Clippy
counts the function body itself as one nesting level, so a limit of 3 control-flow levels is
configured as `excessive-nesting-threshold = 4`.

`cargo clippy --all-targets` MUST pass with no warnings. A limit crossed is a warning, and a
warning is a Blocker.

**The escape hatch.** A limit MAY be exceeded when the alternative is worse — splitting a
cohesive state machine across three files makes it harder to read, not easier. Exceeding a limit
MUST be justified in place with an allow attribute whose reason cites a requirement ID or ADR:

```rust
// FR-004: the profile is one contiguous template; splitting it hides the shape of the output.
#[allow(clippy::too_many_lines, reason = "FR-004")]
fn sandbox_profile(shared: &[PathBuf], proxy_port: u16) -> String {
```

A bare `#[allow]` with no reason is a Blocker. The escape hatch is for a named trade-off, not for
silencing the lint.

## 3. One purpose

Every function, module, package and component MUST serve exactly one purpose.

**The test:** name what it does in one sentence, without "and". If the sentence needs "and", or
needs a comma splice, the unit does two things — split it.

- `config.rs` loads configuration. It does not also validate paths against the filesystem.
- `sandbox.rs` builds a profile and launches under it. If launching grows its own error
  handling, retries and signal plumbing, it has become a second purpose and moves out.

A module's name is a claim about its purpose. Code that does not match the name belongs
elsewhere; a module named `utils`, `helpers`, `common` or `misc` is a purpose that was never
decided, and MUST NOT be created.

## 4. Evidence over anticipation

Do not anticipate. Use fact to prove.

An edge case, a configuration knob, a trait boundary or an error variant earns code only when at
least one of these holds:

1. A requirement in `docs/specs/` calls for it — cite the `FR-`/`NFR-` ID.
2. A reproducible failure demonstrates it — cite the reproduction.
3. A failing test demands it — the test exists and is red before the code is written.

If none holds, the case is speculation. Under `docs/guidelines/sdd/spec-driven-development.md`
speculation belongs in **Non-goals** or **Open questions**, not in `src/`.

**"What if someone later wants…"** is not evidence. Neither is "it costs nothing to support" —
it costs a branch that must be read, a state that must be reasoned about, and a test that must
be written or a gap that must be excused.

Deleting speculative code is cheap; the git history keeps it. Carrying it is not.

## 5. Over-engineering smells

Each row is a Blocker when the trigger holds and no §4 evidence is cited.

| Smell | Trigger | Do instead |
| --- | --- | --- |
| Trait with one implementor | The trait has exactly one `impl` and no test double requires it | Use the concrete type |
| Generic with one instantiation | The type parameter resolves to one type across the crate | Name the concrete type |
| `Box<dyn Trait>` with one type | Only one type is ever boxed | Store the concrete type |
| Builder for a small struct | The struct has 3 or fewer fields | Struct literal, or `new(a, b)` |
| Premature `async` | Nothing in the call chain awaits concurrently | A synchronous `fn` |
| Unused config knob | No `FR-`/`NFR-` requires it to be configurable | Hardcode the value |
| Forwarding wrapper | Every method delegates and adds nothing | Delete it; use the inner type |
| Unconstructed variant | An `enum` variant is never built (`dead_code` fires) | Delete the variant |
| Impossible error case | The error cannot occur given the caller's invariants | `expect("why")`, or restructure so it is unrepresentable |
| Redundant optionality | `Option<Vec<T>>` where empty and absent mean the same | `Vec<T>` |
| Speculative layer | A module or trait exists for exactly one caller | Inline it at the call site |

**Rule of three.** Do not abstract on the second occurrence. Duplicate it, wait for the third,
then abstract with three real cases in hand. Two data points describe a line through anything;
the abstraction extracted from them is usually the wrong shape.

## 6. Idiomatic Rust

Use the common pattern for a common problem. A reader who knows Rust should not have to learn
this repository's private vocabulary.

- Prefer the standard library and the crates already in `Cargo.toml` over a hand-rolled
  equivalent. Reach for a new dependency only per `AGENTS.md` — well known, well maintained.
- Prefer iterator adapters over index loops; `?` over a `match` that only re-wraps a `Result`;
  `if let` / `let else` over nested `match`; `impl Trait` in argument position over a named
  generic used once.
- Errors follow `docs/guidelines/code/rust.md` §5: one `thiserror` enum, one variant per distinct
  failure a caller can act on.
- Derive rather than implement: `Debug`, `Clone`, `Default`, `PartialEq` when the derived
  behaviour is correct.
- Name things after what they are in the domain (`shared_paths`, `proxy_port`), not after their
  shape (`string_vec`, `data`, `info`).
- Comments explain **why**. Code needing a comment to explain **what** SHOULD be rewritten.

## 7. Review checklist

- [ ] `cargo clippy --all-targets` passes with no warnings.
- [ ] Every `#[allow]` cites a requirement ID or ADR in a `reason`.
- [ ] Every file is under 400 lines.
- [ ] Every function, module and component passes the one-sentence-without-"and" test (§3).
- [ ] No module named `utils`, `helpers`, `common` or `misc`.
- [ ] Every branch handling an edge case cites evidence per §4.
- [ ] No smell in §5 fires without a cited justification.
- [ ] No abstraction introduced with fewer than three real call sites (§5, rule of three).

## 8. Related

- `docs/guidelines/code/rust.md` — module docs, rustdoc, the crate error type.
- `docs/guidelines/code/consistency.md` — reuse before writing; one name per concept.
- `docs/guidelines/code/quality-gates.md` — running clippy, and what may not be silenced.
- `docs/guidelines/architecture/posix.md` — the CLI surface these rules produce.
- `docs/guidelines/sdd/spec-driven-development.md` — where evidence (§4) comes from.
