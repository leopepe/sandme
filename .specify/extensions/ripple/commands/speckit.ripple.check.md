---
description: "Re-verify ripple report items and update resolution status"
scripts:
  sh: scripts/bash/check-prerequisites.sh --json --paths-only
  ps: scripts/powershell/check-prerequisites.ps1 -Json -PathsOnly
---

# Ripple Check

## Why This Exists

After `/speckit.ripple.scan` generates a report, the developer addresses findings — fixing code, adding safeguards, or accepting risks. This command re-verifies each finding against the current codebase to confirm whether issues have actually been resolved, or if new side effects have been introduced by the fixes themselves.

## User Input

```text
$ARGUMENTS
```

You **MUST** consider the user input before proceeding (if not empty).

Parse the user's input for optional filters:

| Keyword | Behavior |
|---------|----------|
| _(default)_ | Re-check all findings awaiting verification (`OPEN`, `RESOLUTION_PLANNED`, `MITIGATED`, `WORSENED`) |
| `critical` | Re-check only CRITICAL findings |
| `R-{NNN}` | Re-check specific finding(s) by ID — including terminal ones |

Examples:
- `/speckit.ripple.check` — re-check all non-terminal findings
- `/speckit.ripple.check critical` — re-check critical findings only
- `/speckit.ripple.check R-001 R-005` — re-check specific findings

## Workflow

### Step 1: Load Context

Run `{SCRIPT}` from the repository root and parse `FEATURE_DIR` from the JSON output. (If the `{SCRIPT}` placeholder was not substituted at install time, run `.specify/scripts/bash/check-prerequisites.sh --json --paths-only`, or on Windows `.specify/scripts/powershell/check-prerequisites.ps1 -Json -PathsOnly`.) Then load:

- **Required**: `ripple-report.md` (from a previous scan)
- **Required**: `tasks.md`, `spec.md`
- **Optional**: `plan.md`
- **Optional**: `blueprint.md` (produced by the spec-kit-blueprint companion extension, if installed)
- **Optional**: `ripple-fixes.md` (fix plans from `/speckit.ripple.resolve` — refreshed in Step 5)

If `ripple-report.md` is missing, abort with: "Run `/speckit.ripple.scan` first."

### Step 2: Parse Existing Findings

Parse `ripple-report.md` and extract all findings whose status is **non-terminal**: `OPEN`, `RESOLUTION_PLANNED`, `MITIGATED`, or `WORSENED`. Terminal statuses (`RESOLVED`, `ACCEPTED_RISK`, `STALE`) are skipped unless the user names them explicitly via an `R-{NNN}` filter.

For each finding, capture:

- Finding ID (R-{NNN})
- Category
- Severity
- The `Affected` anchor (file path, symbol, approximate line)
- Description of the side effect
- Recommendation
- Resolution Strategy (present on `RESOLUTION_PLANNED` findings)

Apply the user's filter if provided (severity or specific IDs).

### Step 3: Re-verify Each Finding

For each finding in scope:

1. **Read the current file** at the referenced path and locate the referenced **symbol** — line numbers may have drifted since the scan; the symbol anchor is authoritative.
2. **See what changed since the scan**: use the report's `**Scanned-Commit**` header — `git diff {Scanned-Commit} -- {path}` shows how the file evolved (including uncommitted edits). If the report has no such header (pre-1.1.0 scan), skip this diff and judge from the current code and the finding's description. Judge from the current code, not the diff alone.
3. **Evaluate resolution**:

| Verdict | Condition | New Status |
|---------|-----------|------------|
| **RESOLVED** | The side effect is no longer present — code was fixed or safeguard was added | RESOLVED |
| **MITIGATED** | Risk reduced but not eliminated — partial fix or documented acceptance | MITIGATED |
| **OPEN** | The side effect still exists unchanged | OPEN |
| **WORSENED** | The fix attempt introduced additional problems or the original issue expanded | WORSENED |
| **STALE** | The referenced file/line no longer exists (deleted or heavily refactored) | STALE |

For findings entering as `RESOLUTION_PLANNED`: if the planned fix was implemented and eliminates the side effect → RESOLVED; implemented but ineffective or harmful → WORSENED (describe what happened); not implemented at all → back to OPEN with the note "planned fix not yet implemented" (keep the recorded strategy).

4. **For WORSENED findings**: Describe what changed and what new risk was introduced
5. **For STALE findings**: Attempt to locate the code in its new location; if found, re-evaluate; if not found, mark as STALE with a note

### Step 4: Scan for Fix-Induced Side Effects

The fixes themselves can introduce new ripple effects. For each finding whose status moved to RESOLVED or MITIGATED (from any non-terminal status — OPEN, RESOLUTION_PLANNED, MITIGATED, or WORSENED):

1. Identify what code was changed to address the finding
2. Run the fix through the 9 ripple categories, scoped narrowly to the changed lines. Compact rubric (see `/speckit.ripple.scan` for the full expansions):
   - **Data Flow** — shape/type/encoding/validity of data in motion changed; persisted data no longer round-trips
   - **State & Lifecycle** — new shared-state mutation, acquisition without release, init-order dependency, stale cache
   - **Interface Contract** — call-site contract changed: signature semantics, return meaning, pre/postconditions, events, access control
   - **Resource & Performance** — complexity, hot-path allocations, I/O frequency, or batching changed
   - **Concurrency** — new races, broken atomicity, lock ordering, async completion-order assumptions
   - **Distributed Coordination** — new network-call assumptions, lost idempotency, cross-service ordering/consistency gaps
   - **Configuration & Environment** — new config keys, deploy ordering, rollback safety, relaxed security config
   - **Error Propagation** — new failure modes, changed error types, swallowed errors, partial-failure states
   - **Observability** — lost logs/metrics/trace context, broken dashboards, stale health checks
3. If new side effects are found, add them as new findings with `**Status**: OPEN` and the note: "Introduced while resolving R-{original}". New IDs continue the report's monotonic sequence (highest existing R-{NNN} + 1)

### Step 5: Update ripple-report.md

Update the existing `ripple-report.md` in place:

1. **Update finding statuses** — change each finding's non-terminal status (`OPEN`, `RESOLUTION_PLANNED`, `MITIGATED`, `WORSENED`) to `RESOLVED`, `MITIGATED`, `WORSENED`, or `STALE` as determined (or back to `OPEN` when a planned fix is missing)
2. **Add resolution notes** — for each status change, append:

```markdown
- **Status**: RESOLVED
- **Resolution**: {What was done to fix it} ({date})
```

3. **Append new findings** — if fix-induced side effects were found, add them to the appropriate severity section with new monotonic IDs and status OPEN
4. **Update header lines** — update only the `**Status**:` line to reflect new statuses. The severity `**Findings**:` counts are cumulative: add new fix-induced findings to their severity counts, never decrement for resolved items
5. **Coverage Gap Matrix** — the matrix counts findings by severity regardless of status: do not remove resolved items; add new fix-induced findings to their category cells
6. **Update Check History** — append a row to the existing table, never create a duplicate section:

   - **If `## Check History` section already exists**: Find the existing table and append a new row at the bottom. Do NOT create a second `## Check History` heading.
   - **If `## Check History` section does not exist**: Create it once at the bottom of the report with the table header and the first row.
   - If a `### Check detail` or `### Implementation detail` sub-section exists from a prior check, leave it intact. Add a new sub-section for this check session below it.

```markdown
## Check History

| Date | Scope | Resolved | Mitigated | Worsened | Stale | New | Still Open |
|------|-------|----------|-----------|----------|-------|-----|------------|
| {prior rows unchanged} | | | | | | | |
| {datetime} | {all/critical/R-NNN} | {count} | {count} | {count} | {count} | {count} | {count} |
```

**Deduplication rule**: Before writing, scan the report for existing `## Check History` headings. If more than one exists (from a prior bug), merge all rows into a single table under one heading and remove the duplicate.

7. **Refresh `ripple-fixes.md`** (if it exists): regenerate it as a derived view of the updated report — keep only the plan entries for findings whose status is still `OPEN` or `RESOLUTION_PLANNED`; drop all others (their plans are finished, failed, or accepted). Update the header to `# Ripple Fixes — regenerated {date}` (this run's date) and keep the note `> Derived from ripple-report.md — fix plans still awaiting application. Status lives in the report, not here.` The file carries no status of its own — `ripple-report.md` is the sole status authority. If no plans remain, keep the header and write: "No fix plans awaiting application — see ripple-report.md for current finding status."

### Step 6: Report

Output a summary:
- Total findings checked
- Status changes: {N} resolved, {N} mitigated, {N} worsened, {N} stale, {N} still open
- New findings introduced by fixes (if any)
- **Convergence verdict**, computed over the full updated report — including fix-induced findings added in Step 4 — from the findings that still demand action (`OPEN` + `RESOLUTION_PLANNED` + `WORSENED`):
  - Zero → "CONVERGED — no findings awaiting action; the report is stable." (List MITIGATED findings separately, as accepted partial fixes.)
  - Otherwise → "NOT CONVERGED — {N} findings awaiting action ({breakdown by severity and status}). Continue the loop: resolve → implement → check."
- Critical-path line, keyed on the same action set as the verdict and clearly scoped so it is never read as full convergence: "All CRITICAL findings resolved or accepted — critical path clear." or "{N} critical findings still awaiting action. Address before merging."
- If WORSENED findings exist: highlight them prominently and suggest `/speckit.ripple.resolve` to plan a new fix
- **Coverage warning**: compare the files changed since the scan (`git diff {Scanned-Commit} --name-status` plus untracked files) against the files examined in Steps 3–4. If some were not covered: "{N} changed files were not covered by fix-induced analysis — run `/speckit.ripple.scan --diff` to scan them." If the report has no `**Scanned-Commit**` header (pre-1.1.0 scan), skip this warning — the change set since the scan cannot be reconstructed.

## Rules

- **Never delete findings**: Findings change status but are never removed from the report. This preserves audit history.
- **Evidence for every verdict**: Each status change must reference what specifically changed in the code. No status changes based on assumptions.
- **Fix-induced analysis is mandatory**: Skipping Step 4 defeats the purpose. Fixes that introduce new problems are common — always check.
- **WORSENED is serious**: If a fix made things worse, flag it prominently. Do not bury it in the report.
- **ACCEPTED_RISK is respected**: The user explicitly accepted these risks — do not re-check or re-litigate them unless named by ID.
- **Preserve finding IDs**: Never renumber existing findings. New findings get the next monotonic ID (highest R-{NNN} in the report + 1).
- **Read before judging**: Always read the current state of the file before changing a finding's status. Do not rely on git diff alone — the full context matters.
- **Scope honesty**: If the check was filtered (e.g., `critical` only), state clearly in the report that unchecked findings retain their previous status.
- **Language of the report**: Follow the language used in the existing `ripple-report.md`.
