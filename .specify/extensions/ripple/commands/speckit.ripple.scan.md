---
description: "Analyze implementation for untested side effects and generate ripple-report.md"
scripts:
  sh: scripts/bash/check-prerequisites.sh --json --require-tasks --include-tasks
  ps: scripts/powershell/check-prerequisites.ps1 -Json -RequireTasks -IncludeTasks
---

# Ripple Scan

## Why This Exists

Tests verify that code does what it should. But they rarely catch what code does *besides* what it should. After implementation, changes ripple outward — affecting data flows, breaking implicit contracts, shifting timing assumptions, or introducing resource pressure that no test was designed to detect.

This command is **NOT a general code review**. It answers one specific question: **"What did this implementation break or put at risk that wasn't broken before?"** Every finding must be causally linked to a specific change made during implementation. Pre-existing issues that weren't affected by the changes are out of scope.

## User Input

```text
$ARGUMENTS
```

You **MUST** consider the user input before proceeding (if not empty).

Parse the user's input for optional arguments:

| Keyword | Behavior |
|---------|----------|
| _(default)_ | Report all findings (critical, warning, info) |
| `critical` | Only report critical-severity findings |
| `--diff` | Incremental scan — only files changed since the last scan |
| `--base <ref>` | Compare against `<ref>` instead of the auto-detected base branch |

Examples:
- `/speckit.ripple.scan` — full scan, all severities
- `/speckit.ripple.scan critical` — critical issues only
- `/speckit.ripple.scan --diff` — incremental scan on changed files
- `/speckit.ripple.scan --base develop` — feature was branched off `develop`

## Workflow

### Step 1: Load Context

Run `{SCRIPT}` from the repository root and parse `FEATURE_DIR` and `AVAILABLE_DOCS` from its JSON output. (If the `{SCRIPT}` placeholder was not substituted at install time — older Spec Kit CLIs don't render it for extension commands — run `.specify/scripts/bash/check-prerequisites.sh --json --require-tasks --include-tasks`, or on Windows `.specify/scripts/powershell/check-prerequisites.ps1 -Json -RequireTasks -IncludeTasks`.) The script validates that `tasks.md` exists — if it reports an error, surface it and stop.

Then load from `FEATURE_DIR`:

- **Required**: `tasks.md`, `spec.md`, `plan.md`
- **Optional** (when listed in `AVAILABLE_DOCS`): `data-model.md`, `contracts/`, `checklists/`
- **Optional**: `blueprint.md` (produced by the spec-kit-blueprint companion extension, if installed)
- **Optional**: `ripple-report.md` (previous scan — merged into, never overwritten; see Step 5)

### Step 2: Establish Baseline and Identify Changes

**This is the most critical step.** Ripple analysis is delta-anchored — it compares "before" vs. "after" to find side effects introduced by the implementation. Without a clear baseline, the analysis degenerates into a general code review.

#### 2a: Determine the Baseline

Resolve the base branch, then the branch point:

1. If the user passed `--base <ref>`, use that ref.
2. Otherwise run `git symbolic-ref --quiet --short refs/remotes/origin/HEAD` to get the remote's default branch (e.g. `origin/main`).
3. If that returns nothing, probe `origin/main`, `origin/master`, `origin/develop`, `main`, `master` in order and use the first ref that `git rev-parse --verify --quiet <ref>` confirms exists.

Compute the baseline commit — referred to as **BASE** below:

```bash
git merge-base HEAD {resolved-ref}
```

**Guard**: if no ref resolves, or `git merge-base` fails or returns nothing, ABORT with: "ERROR: could not determine a baseline (no main/master/develop found and no `--base` given). Re-run with `--base <ref>`." Never analyze without a before-state.

BASE represents the state of the codebase **before** implementation. All analysis must be relative to this point.

#### 2b: Extract the Change Set

The after-state is the **working tree**, not HEAD — `/speckit.implement` typically leaves changes uncommitted. Never restrict the diff to committed history (`..HEAD`).

Run both:

```bash
git diff --name-status -M -C {BASE}           # committed + staged + unstaged changes vs. baseline
git ls-files --others --exclude-standard      # untracked files — treat as status A
```

This produces the **change set** — the files changed by the implementation. They are the **cause** of potential ripple effects. Status letters: `A` added, `M` modified, `D` deleted, `T` type change, plus `R`/`C` (rename/copy — a three-field row `R100 old/path new/path`; analyze the **new** path, and treat the move itself as a potential Interface Contract ripple for anything that still references the old path).

**Empty guard**: if both commands return nothing, STOP and report: "No changes detected relative to {BASE}. Confirm you are on the feature branch and the implementation exists on disk." Do NOT generate a report from an empty diff.

Cross-reference the change set with `tasks.md` to understand the **intent** behind each change.

**`--diff` (incremental) mode** — anchors on the previous report's `**Scanned-Commit**` header:

1. If the report has no `**Scanned-Commit**` header (generated by a pre-1.1.0 scan), `--diff` cannot anchor an incremental set — fall back to a full scan and note in the report: "previous report predates commit-anchored diffs; ran a full re-scan."
2. Read the recorded SHA and verify it still exists: `git cat-file -e {sha}^{commit}`. If it doesn't (e.g. rebased away), fall back to a full scan and note why in the report.
3. Incremental set = `git diff --name-status -M -C {sha}` plus `git ls-files --others --exclude-standard` (untracked files are invisible to `git diff` — include them so new files created since the last scan survive the intersection below). This is immune to rebase/amend timestamp rewrites and sees uncommitted edits. Files committed unchanged since the last scan may re-appear; re-analyzing them is safe — missing them is not.
4. Analyze only files present in BOTH the change set from 2b AND the incremental set.
5. Carry over all other findings from the previous report unchanged.
6. If the incremental set is empty, report: "No changes since last scan ({short sha}). Run without `--diff` for a full re-scan." and stop.

#### 2c: Identify the Blast Radius

For each file in the change set, find its **dependents** — files outside the change set that consume it:

1. From each diff hunk, collect the changed exported symbols (function/class/constant names) and the module's import path or basename.
2. Search the repository for those symbols and import paths using language-appropriate patterns (`import`, `require`, `from`, `use`, `#include`, configuration references), excluding the change set itself.
3. **Read each candidate file.** A Blast Radius entry is valid only if you opened the dependent and confirmed it consumes the changed behavior. Never list a file you did not read.

The blast radius also includes **non-code dependents** the diff puts at risk: data persisted by the prior version that the new code must still read, caches holding old-format values across a deploy, in-flight queue messages, and old-version instances running during a rolling deploy.

These dependents are where ripple effects manifest. Read them to understand what assumptions they make about the changed code. Trace at least one level of dependents for every changed file; when a finding looks CRITICAL, trace a second level (the dependents of the dependents).

#### 2d: Read the Diffs

For each changed file, read the actual diff to understand **what specifically changed**:

```bash
git diff -M -C {BASE} -- {file_path}
```

For untracked files, read the whole file as added content. For binary files (shown as `-	-` in `git diff --numstat {BASE}`), record the add/modify/delete at file granularity and state that hunk-level tracing is not possible — never fabricate line-level causes. Report a submodule pointer change as a Configuration & Environment concern (coordinated-deploy risk), not as hunk analysis.

Understanding the specific change is essential. A finding must trace back to a specific change — a diff hunk for line-level edits, or the file-level add/modify/delete/rename/submodule change where no hunks exist (binary files, pure renames, submodule bumps) — not just "this file was modified."

### Step 3: Analyze Across 9 Categories

For each change in the diff, trace its impact on the blast radius files. The analysis follows a strict causal chain:

```
Specific change (diff hunk) → Affected dependent (blast radius file) → Side effect (what breaks or becomes risky)
```

**Causation test**: Before reporting a finding, ask: *Would this problem exist in its current form and severity if the change had not been made?* If it existed and the change did not affect it — it is pre-existing, do NOT report it. If the change introduced it OR measurably worsened it — report it, stating the before/after severity delta.

Work through all 9 categories for each change. Finding nothing in a category is the normal outcome for most diffs — move on; never invent a finding to fill a category. **Zero findings overall is a valid, successful result.**

---

#### Category 1: Data Flow

Trace how the change altered the way data enters, transforms, and exits the code.

**Look for changes that:**
- Altered the shape, type, or encoding of data that downstream consumers still expect in the old format
- Added/removed fields in a serialization path without updating the deserialization counterpart
- Changed a persisted format or schema so that data written **before** this change no longer round-trips (deserializes to null/defaults, or throws)
- Introduced implicit type coercion or precision loss that didn't exist before
- Changed what happens to invalid/missing data (e.g., previously rejected, now silently default-filled)
- Broke an assumption a dependent module had about data ordering, nullability, or completeness
- Exposed sensitive data (credentials, PII, tokens) to a new output path — logs, responses, or external services — that didn't receive it before

---

#### Category 2: State & Lifecycle

Examine how the change introduced new state mutations or altered object lifetimes.

**Look for changes that:**
- Introduced new shared/global state mutation that outlives the operation's intended scope
- Added resource acquisition (handles, connections, locks) on a code path introduced or changed by this diff without a corresponding release — only if the leak is new or worsened; do not flag pre-existing leaks in touched files
- Created a new initialization order dependency that didn't exist before
- Removed or reordered lifecycle hooks/teardown logic that dependents relied on
- Changed the data a specific cache or memo is derived from, leaving its entries stale — name the cache

---

#### Category 3: Interface Contract

Check whether the change altered — explicitly or implicitly — the contract that other modules depend on. (Boundary with Data Flow: Interface Contract = the call-site contract changed — signature, pre/postconditions, return semantics; Data Flow = the shape/content/validity of the data itself changed.)

**Look for changes that:**
- Modified a method/function signature in a way that compiles but shifts semantics for callers
- Changed the meaning of a return value (same type, different interpretation now)
- Altered preconditions/postconditions that callers were relying on without updating them
- Broke an implicit contract (e.g., "this always returned sorted results" but now it doesn't)
- Changed event/callback emission — different order, different payloads, or stopped emitting entirely
- Weakened an access control contract — operations that previously required authorization now don't, or permission checks that were bypassed or downgraded

---

#### Category 4: Resource & Performance

Assess whether the change altered resource consumption or performance characteristics compared to before.

**Look for changes that:**
- Increased loop/recursion depth or changed scaling behavior with input size
- Added allocations inside hot paths that didn't exist before (per-request, per-item, per-tick)
- Moved I/O operations into a tighter loop or increased call frequency
- Changed algorithmic complexity (e.g., was O(n), now O(n^2) due to a newly nested lookup)
- Broke a batch/bulk operation into individual calls, or vice versa, affecting throughput

---

#### Category 5: Concurrency

Analyze whether the change introduced new thread-safety, async, or parallel execution risks within a single process.

**Look for changes that:**
- Exposed shared mutable state to concurrent access that was previously single-threaded or protected
- Broke an atomicity assumption (e.g., a read-then-write that was safe before but now races)
- Introduced new lock acquisition that conflicts with existing lock ordering
- Made async operations depend on completion order without enforcing it
- Moved a callback or handler to run on a different thread/context than before

---

#### Category 6: Distributed Coordination

Evaluate whether the change introduced new cross-process, cross-service, or cross-node risks.

**Look for changes that:**
- Added new network calls that assume success or instant response where there were none before
- Removed or omitted idempotency on operations that can now be retried (message redelivery, API retry)
- Introduced new ordering assumptions across service boundaries that aren't enforced
- Created a consistency gap — updated one side of a service boundary without the other
- Changed behavior when a downstream dependency is unreachable (previously handled, now not)
- Extended a transaction boundary to span multiple services without compensation/rollback logic

---

#### Category 7: Configuration & Environment

Check whether the change introduced new configuration or deployment requirements.

**Look for changes that:**
- Added new environment variables, config keys, or feature flags without documenting or defaulting them
- Introduced environment-specific behavior (dev vs. staging vs. production) that isn't accounted for
- Changed dependency versions that require coordinated deployment — or that silently change the behavior of unmodified code that calls them
- Added new files or modules not included in build/package configuration
- Created new migration or deployment ordering requirements not captured anywhere
- Made the deploy non-rollback-safe or created a mixed-version window — new-format data the prior version can't read, or old/new instances exchanging incompatible data during rollout
- Relaxed security-related configuration (access controls, trust boundaries, encryption settings, rate limits) that was stricter before

---

#### Category 8: Error Propagation

Trace how the change altered error flows compared to the previous behavior.

**Look for changes that:**
- Introduced new failure modes without corresponding error handling in callers
- Changed error types or codes that upstream catch/match logic depends on
- Added catch blocks that swallow errors silently where errors previously propagated
- Created new partial failure states where the operation can half-complete, leaving inconsistent state
- Added retry logic on operations that aren't idempotent
- Changed error messages or codes that other components parse programmatically
- Altered error responses to include implementation internals (debug details, query information, system paths) that weren't exposed before

---

#### Category 9: Observability

Assess whether the change degraded the ability to monitor, debug, or diagnose issues compared to before.

**Look for changes that:**
- Removed or downgraded log statements for operations that previously had them
- Added new code paths with no logging, metrics, or tracing where parallel paths had them
- Failed to propagate correlation/trace IDs through newly introduced call chains
- Changed metric labels or dimensions, breaking existing dashboards/alerts
- Didn't update health check or readiness probe logic to reflect newly added dependencies
- Lost debug information (stack traces, context) by wrapping errors differently than before

---

### Step 4: Assign Severity

For each finding, assign a severity:

| Severity | Criteria |
|----------|----------|
| **CRITICAL** | Could cause data loss, security breach, or system outage in production |
| **WARNING** | Likely to cause bugs, degraded performance, or operational issues |
| **INFO** | Potential concern worth reviewing — may be intentional or low-risk |

Severity reflects worst-case impact **if the side effect is real**; confidence is tracked separately. If you cannot confirm a suspected side effect actually fires, do not drop it and do not downgrade it — assign severity by impact-if-real and add a `Confidence: low` line to the finding stating what evidence you could not obtain. INFO is for confirmed side effects whose blast radius is trivial or already guarded — never a stand-in for uncertainty.

### Step 5: Generate ripple-report.md

Create `specs/{feature}/ripple-report.md` — or **merge into it** if it already exists:

- Preserve every existing finding, its ID, its current status, and all Resolution History / Check History sections.
- A full re-scan re-evaluates existing OPEN findings (update them in place) and **appends** newly discovered findings. It MUST NOT reset any non-OPEN finding back to OPEN, renumber anything, or drop history.
- Finding IDs are globally monotonic across all commands and modes: next ID = highest `R-{NNN}` present in the report + 1, never reused. Start at `R-001` only when no previous report exists.
- If the scan ran with the `critical` filter, add a header note: `> critical-only scan — WARNING/INFO not analyzed; run a full scan to complete the matrix.`
- Inversely, when a full scan (no `critical` filter) merges into a report carrying that note, remove the note and replace every "—" matrix cell with a real count now that WARNING/INFO have been analyzed.

For the header's `**Branch**` field, use `git rev-parse --abbrev-ref HEAD`; if it returns `HEAD` (detached), use the short commit SHA (`git rev-parse --short HEAD`).

````markdown
# Ripple Report: {Feature Name}

**Branch**: `{branch}` | **Scanned**: {datetime}
**Baseline**: `{BASE short hash}` (branch point from {base branch})
**Scanned-Commit**: `{full HEAD SHA at scan time}`
**Change Set**: {N} files changed | **Blast Radius**: {M} dependents checked
**Findings**: {critical} critical, {warning} warning, {info} info (cumulative — never decremented)
**Status**: {open} open, {planned} planned, {accepted} accepted-risk, {resolved} resolved, {mitigated} mitigated, {worsened} worsened, {stale} stale

## Summary

{2-3 sentence overview of the most significant findings}

## Findings

### CRITICAL

#### R-{NNN}: {Brief title}

- **Category**: {exactly one primary category}
- **Cause**: {What specific change (file + diff hunk) introduced this side effect}
- **Affected**: `{path/to/affected/file}` › `{symbol}` (≈line {N}) — the code that is now at risk
- **Blast Radius**: `{other_affected_1}`, `{other_affected_2}`
- **Before**: {How this behaved before the change — grounded in the removed hunk lines or the BASE version}
- **After**: {How this behaves now — and why that's a problem}
- **Why Tests Miss It**: {The concrete test gap — checked against the actual test suite}
- **Confidence**: low — {what could not be verified} _(include this line only when you could not confirm the effect fires)_
- **Recommendation**: {Concrete action to mitigate}
- **Status**: OPEN

---

### WARNING

#### R-{NNN}: {Brief title}

{same structure as above}

---

### INFO

#### R-{NNN}: {Brief title}

{same structure as above}

---

## Coverage Gap Matrix

| Category | Critical | Warning | Info | Not Applicable |
|----------|----------|---------|------|----------------|
| Data Flow | {count} | {count} | {count} | |
| State & Lifecycle | {count} | {count} | {count} | |
| Interface Contract | {count} | {count} | {count} | |
| Resource & Performance | {count} | {count} | {count} | |
| Concurrency | {count} | {count} | {count} | |
| Distributed Coordination | {count} | {count} | {count} | |
| Configuration & Environment | {count} | {count} | {count} | |
| Error Propagation | {count} | {count} | {count} | |
| Observability | {count} | {count} | {count} | |

> The matrix counts all findings by severity regardless of status. Mark a category "N/A" only after confirming the system has no such surface at all (e.g. no cross-process or cross-service boundary anywhere in the project), and state the specific reason. In a `critical`-filtered scan, fill WARNING/INFO cells with "—" (not analyzed), never 0.

## Next Steps

- [ ] Address CRITICAL findings before merging
- [ ] Review WARNING findings with the team
- [ ] Run `/speckit.ripple.check` after fixes to verify resolution
````

As a finding moves through its lifecycle, `resolve` appends a `**Resolution Strategy**` line and `check` appends a `**Resolution**` line to the finding block — a mature finding carries the base fields above plus these.

### Step 6: Report

Output a summary:
- Path to the generated `ripple-report.md`
- Finding counts by severity
- Up to 3 highest-risk findings highlighted
- Suggested next step (e.g., "Address 2 CRITICAL findings, then run `/speckit.ripple.check`")

## Rules

- **Delta-anchored (MOST IMPORTANT)**: Every finding MUST be causally linked to a specific change in the implementation diff. If a problem existed before the change and the change did not affect it, do NOT report it; if the change introduced or measurably worsened it, report it. Pre-existing issues, general code smells, and hypothetical future problems are out of scope. This is not a code review.
- **Before/After grounded in evidence**: Each finding must describe behavior BEFORE and AFTER the change, and the before-state must be grounded in code you actually read — the removed (`-`) hunk lines or the baseline version via `git show {BASE}:{path}`. If you cannot ground both states, it is a general observation, not a side effect — do not report it.
- **Evidence-based**: Every finding must reference both the causing change (diff hunk — or the file-level add/delete/rename/submodule bump where no hunks exist) AND the affected code (file, symbol, approximate line). No vague warnings.
- **Why Tests Miss It, concretely**: Before writing this field, search the test suite for the changed symbols. If a test already covers the side effect, drop the finding — the goal is to find what tests *miss*. Otherwise name the concrete gap (fixture too small, dependency mocked away, boundary not exercised, timing/order not reproduced) — never just "untested".
- **One primary category per finding**: Assign exactly one category — the *mechanism* of the side effect; mention secondary lenses in prose only (for the Data Flow / Interface Contract boundary, see Category 3).
- **Zero findings ≠ empty scan**: A zero-finding report is valid only when the diff was non-empty and fully analyzed. An empty or failed diff aborts with an explanation — never a clean report.
- **Status vocabulary (one state machine; each command owns its transitions)**: Non-terminal = `OPEN`, `RESOLUTION_PLANNED`, `MITIGATED`, `WORSENED` (check re-verifies all of these; resolve queues `OPEN`/`WORSENED` by default); terminal = `RESOLVED`, `ACCEPTED_RISK`, `STALE` (skipped unless explicitly named by ID). scan creates findings as OPEN; resolve sets RESOLUTION_PLANNED or ACCEPTED_RISK; check sets RESOLVED, MITIGATED, WORSENED, STALE — or back to OPEN.
- **Merge, never overwrite**: When a previous `ripple-report.md` exists, merge as described in Step 5 — preserve IDs, statuses, and history sections; never renumber or silently drop findings.
- **No false confidence**: If a category cannot be fully analyzed (e.g., no access to runtime behavior), state the limitation explicitly in the report.
- **Project-agnostic categories**: Apply categories based on what the code actually does, not what domain it belongs to. A CLI tool can have concurrency issues; an embedded system can have interface contract problems.
- **Severity honesty**: CRITICAL means "will likely break production." Do not inflate severity for attention.
- **Read before judging**: Before flagging a side effect, read the actual implementation of the affected code AND its dependents. Do not assume behavior from names or signatures alone.
- **Blast radius completeness**: Trace dependents to the depth specified in Step 2c — one level for every changed file, two for CRITICAL findings.
- **Scale-aware analysis**: For large change sets (16+ files), prioritize depth over breadth — focus blast radius tracing on the most structurally significant changes (shared modules, interfaces, configuration) rather than trying to trace every file equally. When the `critical` filter is active, skip WARNING/INFO analysis entirely to conserve context, fill their matrix cells with "—", and add the header note from Step 5.
- **Language of the report**: Follow the language used in existing spec/plan/tasks documents.
