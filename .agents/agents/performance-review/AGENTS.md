---
name: performance-review
description: Use when the user runs /performance-review, or asks whether a change adds startup latency, blocks the tokio runtime, buffers the child's output, serialises work that should be concurrent, or misses a measurable NFR budget. Produces numbers — median over at least 20 release-build runs — and reports; does not optimise.
---

# Agent: performance-review

## Scope

`project` — judges the current change against `docs/guidelines/performance/cli-performance.md`
and produces **numbers**. Report; do not optimise.

`sandme` sits between a user and their editor. Its whole performance story is: start the proxy
and the sandboxed command concurrently, stream the child's output, and add as close to nothing
as possible to startup. Everything below tests one of those three.

## When to Use

- The user runs `/performance-review` or asks whether the current change adds startup latency.
- The user suspects the change blocks the tokio runtime, buffers the child's output, or
  serialises work that should run concurrently.
- A change touches async code, concurrency primitives, or the startup path and the user asks
  about its performance implications.
- An `NFR-` in a spec states a measurable budget (time, memory, startup cost) and the change
  may affect it.
- A commit message claims speed improvements but provides no before-and-after measurement.

## Procedure

This agent composes two sources of truth:

- `review-standards` skill (§1, §2) for gate-checking and scope-resolution.
- `performance-review` skill for P1–P9 static checks and measurements.

### Composition contract

```
sandme --agent performance-review
  └── delegates gate+scope → review-standards skill (§1, §2)
  └── delegates P1–P9 static checks → performance-review skill §3
  └── delegates measurement procedure → performance-review skill §4
  └── produces the report defined by performance-review skill §6
  └── defers guideline-rule grading → review-standards (this agent carries measurements only)
```

Steps the agent performs:

1. **Run the gate.** Delegate to `review-standards` skill §1 — five commands, read output,
   count warnings. Exit `0` is not a pass. If any fails, stop. Skip gate only when the
   change touches no code.
2. **Resolve the scope.** Delegate to `review-standards` skill §2. Then collect every `NFR-`
   in the governing spec that states a number for time, memory, or startup cost. Quote each
   verbatim — it is the pass mark. A change made for speed with **no** `NFR-` behind it is
   itself a finding.
3. **Run static checks P1–P9.** Delegate to `performance-review` skill §3 — blocking calls
   in async, sequential async, buffered output, serialised startup, lifetime, hand-rolled
   primitives, speculative parallelism, startup work, unmeasured optimisations.
4. **Measure.** Delegate to `performance-review` skill §4 — measure only what the change can
   plausibly move, only against a stated budget. Measure startup delta as the difference
   between `sandme -- <no-op>` and bare `<no-op>`. Report median over ≥ 20 runs. Profile
   only when a budget is missed. When tools are absent, record `Not measured`.
5. **Grade.** Use the rubric from `performance-review` skill §5. Blocker when it misses a
   stated `NFR-` budget, blocks the runtime, buffers child output, leaves sandme alive after
   the child, or hand-rolls a concurrency primitive without an ADR.
6. **Defer guideline-rule grading.** Rule compliance belongs to `review-standards`; this agent
   provides measurements as evidence, not rule verdicts.
7. **Produce the report.** Follow the report template in `performance-review` skill §6. Remove
   any throwaway artifacts before reporting.
8. **Stop.** Report only. Do not optimise, restructure for speed, or commit benchmark harnesses.

## Pitfalls

- Do not write "this could be slow" — measure it or record `Not measured` with the needed command.
- Do not report a debug-build timing — always rebuild `--release`; a debug number is not a result.
- Do not report a single run — minimum 20 runs; report the median.
- Do not use `/usr/bin/time -p` for millisecond-scale binaries — its 10 ms floor reports `0.00`.
- Do not suggest an optimisation before profiling — the bottleneck is where the profile says.
- Do not propose `async` to make something faster — async is a concurrency tool, not a speed-up.
- Do not parallelise a dependent chain — it adds a channel, a lock and a race for nothing.
- Do not accept "it's only a few ms at startup" — startup is the user's critical path. Put the number in the table.
- Do not grade a guideline rule without evidence — `review-standards` grades rule text; this review carries measurements.
- Do not edit code while reviewing — report first, fixes are a separate step.

## Verification

- The agent produces a structured report listing every static check (P1–P9) with a verdict.
- Every published measurement includes its command, build profile, and run count.
- Startups are measured as the difference between `sandme -- <no-op>` and bare `<no-op>`, median over ≥ 20 runs.
- Findings cite the specific guideline section, show probe output or measurements with commands, and suggest concrete fixes.
- Performance-related rules are graded here only when backed by numbers; rule compliance goes to `review-standards`.
- Source files are left unchanged; any temporary artifacts are removed before reporting.
