---
name: performance-review
description: Use when the user runs /performance-review, or asks whether the current change adds startup latency, blocks the tokio runtime, buffers the child's output, serialises work that should run concurrently, or meets an NFR- performance budget.
---

# Performance review

Judge the current change against `docs/guidelines/performance/cli-performance.md`, and produce
**numbers**. Report; do not optimise.

`sandme` sits between a user and their editor. Its whole performance story is: start the proxy and
the sandboxed command concurrently, stream the child's output, and add as close to nothing as
possible to startup. Everything below tests one of those three.

**Measured, not felt.** "Feels fast", "should be cheap", "this looks slow" are not results. A
finding about speed carries a measurement or it is not a finding — `cli-performance.md` §1 and
`docs/guidelines/code/simplicity.md` §4.

## Not this skill's job

| Question | Skill that owns it |
| --- | --- |
| Does the change break a rule in `docs/guidelines/`, including `cli-performance.md`? | `review-standards` — it grades rule text; this skill produces evidence |
| Is the structure sound, does it contradict an ADR? | `architecture-review` |
| Is the `NFR-` itself present, numbered and verified in the spec? | `spec-review` — it defers the measurement here |
| Does the code simply not work — wrong output, wrong format, a crash? | No review skill owns runtime correctness. List it under `Defects noticed`, ungraded — but see §4: a binary that aborts early cannot be timed |

## 1. Run the gate first

Run the gate — the five commands and their pass criteria are in
`.agents/skills/review-standards/SKILL.md` §1. **Exit status is not the pass criterion:** clippy
and build exit `0` while printing warnings. If any of the five fails, **stop** and report the
failure. A timing taken from code that does not compile or is red measures nothing.

## 2. Resolve the scope

Resolve which diff is under review with the scope table in
`.agents/skills/review-standards/SKILL.md` §2, and state which case you used.

Then collect the budgets: every `NFR-` in the governing spec that states a number for time,
memory, or startup cost. Quote each one verbatim — it is the pass mark in §4. A change made for
speed with **no** `NFR-` behind it is itself a finding (`cli-performance.md` §1).

## 3. Static checks

Read from the diff. Every row gets a verdict.

| # | Check | Probe | Rule |
| --- | --- | --- | --- |
| P1 | Blocking call inside `async fn` | `rg -n -e 'std::fs::' -e 'std::net::' -e 'thread::sleep' -e '\.blocking_' -e 'block_on' src/`, then check whether the caller is `async` | §3 |
| P2 | `async` that never awaits concurrently | For each `async fn` in the diff, name the concurrent await. If every await is sequential, it should be a synchronous `fn` | §3 |
| P3 | Child output buffered | `rg -n -e 'read_to_end' -e 'read_to_string' -e '\.output\(\)' -e 'wait_with_output' src/` — the child's stdout and stderr must stream as produced | §3 |
| P4 | Startup serialised | The proxy and the child must not block each other's startup. Binding the listener before the spawn is a real dependency — the child needs the port. Waiting for anything past `bind` before spawning is not | §2 |
| P5 | Lifetime | Child exits → proxy shuts down, and `sandme` does not outlive the child. Find the code that does it, or it is a finding | §2 |
| P6 | Hand-rolled primitive | `rg -n -e 'thread::spawn' -e 'Mutex::new' -e 'mpsc::' src/` — a scheduler, pool, lock or event loop written here needs an ADR when a crate already covers it | §4 |
| P7 | Speculative parallelism | Anything parallelised where one side reads what the other writes. A channel, a lock and a race bought nothing | §2 |
| P8 | Startup work | Work on the startup path — parse, config, profile, spawn — that the requested command may not need, or any network read | §6 |
| P9 | Unmeasured optimisation | A change whose justification is speed, with no before-and-after cited in the commit message or the spec | §5 |

## 4. Measure

Measure only what the change can plausibly move, and only against a stated budget or a stated
baseline. Never publish a number without its command, its build profile, and its run count.

**Startup delta** — the number that matters most:

```
cargo build --release
hyperfine --warmup 3 --runs 20 './target/release/sandme -- true' 'true'
```

Without `hyperfine`, do **not** reach for `/usr/bin/time -p`: its resolution is 10 ms, which floors
a millisecond-scale CLI to `0.00` and reads as "free". Time it in-process instead, ≥ 20 runs:

```
python3 - <<'PY'
import statistics, subprocess, time

def median_ms(cmd, runs=20):
    samples = []
    for _ in range(runs):
        start = time.perf_counter()
        subprocess.run(cmd, capture_output=True)
        samples.append((time.perf_counter() - start) * 1000)
    return statistics.median(samples)

sandme = median_ms(["./target/release/sandme", "--", "true"])
bare = median_ms(["true"])
print(f"sandme {sandme:.2f} ms · bare {bare:.2f} ms · overhead {sandme - bare:.2f} ms")
PY
```

Report `sandme -- <no-op>` **minus** the bare no-op — that difference is what `sandme` costs, and
it is the number to compare against the `NFR-` from §2. Report the median, never the best run.
Where the cost is dominated by a platform call the change cannot remove (`sandbox-exec` setup,
`fork`), measure that separately and say so; optimising toward it is wasted work.

**Only when a budget is missed**, profile before saying where the time goes:

```
cargo flamegraph --release -- -- true
```

Name the top frame from the profile. The bottleneck is where the profile says it is
(`cli-performance.md` §5).

**After any fix**, re-measure and report before → after with the same command and run count. An
optimisation with no after-number is speculation.

**When the tool is not installed** — no `hyperfine`, no `cargo-flamegraph` — say so, record the
dimension as `Not measured`, and name the command that would produce it. Do not substitute a
guess, and do not report a debug-profile timing as if it were release.

## 5. Grade

- **Blocker** — misses a stated `NFR-` budget; blocks the runtime; buffers the child's output;
  leaves `sandme` alive after the child; hand-rolls a concurrency primitive with no ADR.
- **Advisory** — a measurable cost with no budget against it, or `async` that should be sync.
- **Not measured** — the tooling is absent, or the change cannot be isolated. Say what is needed.
- **Pass** — checked, with the measurement or the probe output that shows it.
- **N/A** — the change contains nothing the check applies to. Say why.

## 6. Report

Emit exactly these sections, in this order:

```markdown
## Performance review

**Gate:** fmt ✓ · clippy ✓ 0 warnings · build ✓ 0 warnings · test ✓ <N> tests ran · doc ✓
**Scope:** <scope case from review-standards §2> — <N files, +A/-B lines>
**Budgets:** NFR-002 — "<verbatim>" | none stated for this change
**Environment:** release profile · <machine> · <N runs> · hyperfine 1.19

### Verdicts
| # | Check | Verdict | Finding |
| --- | --- | --- | --- |
| P1 | Blocking call in async fn | Blocker | PR1 |
| P4 | Startup serialised | Pass | Listener bound at `src/main.rs:22`, child spawned at `:26` |

### Measurements
| What | Command | Before | After | Budget | Verdict |
| --- | --- | --- | --- | --- | --- |
| Startup overhead | `hyperfine --runs 20 './target/release/sandme -- true' 'true'` | 41.2 ms median | — | ≤ 150 ms (NFR-002) | Pass |

### Findings
#### PR1 — <one line> · `src/path.rs:42` · cli-performance.md §<n>
**Rule:** "<verbatim quote from the guideline>"
**Evidence:** <the probe output or the measurement, with its command>
**Cost:** <the number, or "unmeasured — needs <command>">
**Fix:** <the specific change that clears it>

### Defects noticed
<behaviour bugs seen while measuring — `path:line`, one line each, ungraded. "None" if none.>

### Result
<Blockers: N · Advisories: N · Not measured: N> — <one sentence: within budget, or what must change>
```

## 7. Stop

Report only. Do not optimise, do not restructure for speed, do not commit a benchmark harness.

Measuring is not editing, and §4 requires it. A release build, a throwaway timing script and a
scratch input file are permitted artifacts — **remove them before reporting, and say in the report
what you created and that it is gone.** Source files are left exactly as found; `git status` at the
end matches `git status` at the start.

If the user then asks for fixes, apply them one finding at a time, re-measure (§4), and re-run the
gate before saying the change is clean.

## Red flags

| If you catch yourself... | Do this instead |
| --- | --- |
| Writing "this could be slow" | Measure it, or record `Not measured` and name the command |
| Reporting a debug-build timing | Rebuild `--release`. A debug number is not a result |
| Reporting a single run | Minimum 20 runs; report the median |
| Timing a millisecond-scale binary with `/usr/bin/time` | Its floor is 10 ms — it will report `0.00`. Use the in-process harness in §4 |
| Timing a binary that aborts on the happy path | It measures the abort, not the work. Record `Not measured` and put the defect under `Defects noticed` |
| Suggesting an optimisation before profiling | Profile first — §5. The bottleneck is where the profile says |
| Proposing `async` to make something faster | `async` is a concurrency tool, not a speed-up — §3 |
| Parallelising a dependent chain | It buys nothing and adds a channel, a lock and a race — §2 |
| Accepting "it's only a few ms at startup" | Startup is the user's critical path. Put the number in the table |
| Grading a rule without evidence | `review-standards` grades rule text. This review carries measurements |
| Optimising while reviewing | Report first. Fixes are a separate, requested step |
