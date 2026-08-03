---
name: architecture-review
description: Use when the user runs /architecture-review, or asks whether the current change — commit, worktree, branch or PR — contradicts an ADR, puts code in the wrong module, breaks a boundary between modules, or builds structure this codebase cannot grow into.
---

# Architecture review

Judge the **shape** of the current change: where code lives, what depends on what, and whether it
agrees with the decisions already recorded in `docs/adrs/`. Report; do not fix.

Two standards, both binding:

- **Consistent** — the change looks like the codebase it joins and contradicts no accepted ADR. A
  better structure applied to one module out of five makes the codebase worse, not better.
- **Minimal, and not a dead end** — the simplest structure that satisfies today's accepted
  requirements, arranged so the next accepted requirement extends it instead of replacing it.
  Minimal is the default; "not a dead end" is the constraint that stops the minimal choice being
  a trap. It is not a licence to build for load, users or features that do not exist yet.

## Growth is judged, not anticipated

`docs/guidelines/code/simplicity.md` §4 outranks any structure you would prefer to see. A missing
abstraction is a **finding** only when a spec at `Status: Accepted` or `Implemented` already needs
it. Otherwise it is a **Note**: name the seam that would have to change, and stop. The fix for a
Note is never "add the abstraction now".

## One finding, one authority

This review's authority is, in order: an accepted **ADR**, the governing spec's **Constraints and
dependencies**, then **precedent in `src/`**. If the only thing a finding rests on is a line in
`docs/guidelines/`, it belongs to `review-standards` — put it on the `Deferred` line of the report
and do not grade it here.

That includes `docs/guidelines/architecture/`. The directory shares this skill's name but not its
authority: it holds guideline text, and guideline text is graded by rule, in `review-standards`.
Read it for context; defer anything it convicts.

| Question | Skill that owns it |
| --- | --- |
| Does the change break a rule written in `docs/guidelines/`? | `review-standards` |
| Is a requirement in `docs/specs/` implemented, or do two specs conflict? | `spec-review` |
| Does it block the runtime, cost startup time, miss an `NFR-` budget? | `performance-review` |
| Does the code simply not work — wrong output, wrong format, a crash? | No review skill owns runtime correctness. List it under `Defects noticed`, ungraded |

## 1. Run the gate first

Run the gate — the five commands and their pass criteria are in
`.agents/skills/review-standards/SKILL.md` §1. **Exit status is not the pass criterion:** clippy
and build exit `0` while printing warnings, and `cargo test` reporting `0 passed` verified
nothing. Read the output and count.

If any of the five fails, **stop**. Architecture cannot be judged from code that does not compile
or does not pass its tests.

**Stopping means stopping.** Report the gate output and nothing else:

- No preview of what you expect to find, however hedged.
- No "one thing worth knowing before your demo".
- No naming a dimension you would have graded, even as "context, not a finding".

A partial review under a red gate is the worst of both: the reader treats it as the review, and
the code moves under `cargo fmt` before it is finished. Say the gate is red, say what to run, stop.

Skip this step only when the change touches no code. Re-run the gate after any fix made on the
back of this review, before calling the change clean.

## 2. Resolve the scope

Resolve which diff is under review with the scope table in
`.agents/skills/review-standards/SKILL.md` §2, and state which case you used. Untracked files are
part of the change; `git diff` does not show them.

Then read, before grading anything:

- every file in `docs/adrs/` whose Status is accepted — these are binding;
- `AGENTS.md`, and the nearest `AGENTS.md` to each touched file;
- the governing spec in `docs/specs/` — its **Constraints and dependencies** and **Non-goals**;
- the siblings in `src/` of every touched module — they are the precedent.

## 3. Dimensions

Every row gets a verdict, including the ones that pass.

| # | Dimension | The question | Where the answer comes from |
| --- | --- | --- | --- |
| A1 | ADR conformance | Does the change do something an accepted ADR decided against? | `docs/adrs/*.md` — Decision and Consequences |
| A2 | Undocumented decision | Did the change make a structural choice that is expensive to reverse, with no ADR? | the diff vs. `docs/AGENTS.md` — an ADR was required |
| A3 | Module purpose | After the change, does each touched module still do one thing its name claims? | the `//!` doc vs. what the module now contains |
| A4 | Placement | Is new code in the module whose stated purpose already covers it, or in the module that happened to be open? | sibling modules in `src/` |
| A5 | Dependency direction | Does a module now know about one that should not know about it — `config` aware of the proxy, `error` aware of the sandbox? | the `use` lines in the diff |
| A6 | Second home | Does a responsibility now live in two places — configuration read outside `config.rs`, an error type declared outside `error.rs`? | `rg -n -e 'env::var' -e 'thiserror::Error' -e 'fs::read' src/` |
| A7 | Structural symmetry | Does a new module have the shape of its siblings — `//!` doc, imports, public items, `#[cfg(test)] mod tests`? | the other files in `src/` |
| A8 | One user | Does the change introduce a trait, generic, layer, module or knob with exactly one user? | the diff; count the call sites |
| A9 | Growth seam | For each accepted-but-unimplemented requirement: would it be **added** to this structure, or would it **replace** it? | `docs/specs/` |

A8 is a Blocker when it fires and nothing in `docs/specs/` requires the indirection. A9 is a Note
unless the requirement is already accepted — see *Growth is judged, not anticipated*.

## 4. Grade

- **Blocker** — contradicts an accepted ADR; breaks a module's stated purpose; gives a
  responsibility a second home; makes a hard-to-reverse decision with no ADR; adds structure with
  one user and no requirement behind it.
- **Advisory** — inconsistent with sibling structure, but contained and cheap to correct.
- **Note** — a growth risk with no accepted requirement behind it. Never a Blocker.
- **Pass** — checked, nothing found. Name what you checked.
- **N/A** — the change contains nothing the dimension can apply to. Say why.

## 5. Report

Emit exactly these sections, in this order:

```markdown
## Architecture review

**Gate:** fmt ✓ · clippy ✓ 0 warnings · build ✓ 0 warnings · test ✓ <N> tests ran · doc ✓
**Scope:** <scope case from review-standards §2> — <N files, +A/-B lines>
**Binding decisions read:** <ADR files, spec sections, AGENTS.md files>
**Deferred:** <findings handed to review-standards / spec-review / performance-review, one line each>

### Verdicts
| # | Dimension | Verdict | Finding |
| --- | --- | --- | --- |
| A1 | ADR conformance | Pass | Checked ADR-0001; the proxy stays in its own process |
| A4 | Placement | Blocker | AR1 |

### Findings
#### AR1 — <one line> · `src/path.rs:42-58` · <ADR / spec section / precedent by path:line>
**Binds:** "<verbatim quote from the ADR or spec, or the sibling pattern named by path:line>"
**Evidence:** <what the change actually does>
**Consequence:** <what this costs the next change — concrete, not "harder to maintain">
**Fix:** <the specific move, split or deletion that clears it>

### Growth notes
| Seam | Requirement that would test it | What would have to change |
| --- | --- | --- |
| `Config` is built once at startup | FR-012 (Draft) — reload on SIGHUP | Ownership moves behind a handle; nothing today needs it |

### Defects noticed
<behaviour bugs seen while reviewing — `path:line`, one line each, ungraded. "None" if none.>

### Result
<Blockers: N · Advisories: N · Notes: N> — <one sentence: sound as it stands, or what must move first>
```

## 6. Stop

Report only. Do not edit code, specs or ADRs. If the user then asks for fixes, apply them one
finding at a time, citing the finding ID, and re-run the gate (§1) before saying the change is
clean.

## Red flags

| If you catch yourself... | Do this instead |
| --- | --- |
| Writing "this won't scale" | Name the accepted requirement it fails, or downgrade it to a Note with the seam named |
| Proposing a trait, layer or generic for one caller | That is the Blocker in A8, not the fix for one |
| Citing an ADR from memory | Open `docs/adrs/`, quote the Decision line, cite the file |
| Grading a rule that lives in `docs/guidelines/` | Put it on the Deferred line; `review-standards` owns it |
| Judging structure you did not read | Read the siblings in `src/` first — precedent is the standard |
| Reviewing before the gate, or reading `exit 0` from clippy as a pass | Stop. Count warnings, not exit statuses; the review runs after the gate passes |
| Adding "one thing worth knowing" to a gate-failure report | That is the review, hedged. Report the gate and stop — §1 |
| Redesigning the module while reviewing it | Report first. Fixes are a separate, requested step |
| Reporting zero findings with no verdict rows | Every dimension A1–A9 gets a row, including passes |
