---
description: "Interactively resolve ripple findings one by one — review cause, choose a fix strategy, and record a fix plan"
scripts:
  sh: scripts/bash/check-prerequisites.sh --json --paths-only
  ps: scripts/powershell/check-prerequisites.ps1 -Json -PathsOnly
---

# Ripple Resolve

## Why This Exists

`/speckit.ripple.scan` surfaces side effects. But a list of problems is not a list of solutions. Each finding may have multiple valid resolution strategies with different tradeoffs — quick patch vs. structural fix, defensive guard vs. root cause elimination, accept risk vs. redesign.

This command walks through each finding interactively, presents resolution options with tradeoffs, and records the decision. It turns "here's what's broken" into "here's what we decided to do about it."

## User Input

```text
$ARGUMENTS
```

You **MUST** consider the user input before proceeding (if not empty).

Parse the user's input for optional filters:

| Keyword | Behavior |
|---------|----------|
| _(default)_ | Resolve all findings needing a plan (`OPEN` or `WORSENED`), CRITICAL first |
| `critical` | Resolve only CRITICAL findings |
| `R-{NNN}` | Resolve specific finding(s) by ID — any status |
| `--dry-run` | Show resolution options without recording anything |

Examples:
- `/speckit.ripple.resolve` — walk through all findings needing a plan
- `/speckit.ripple.resolve critical` — critical findings only
- `/speckit.ripple.resolve R-001 R-003` — resolve specific findings
- `/speckit.ripple.resolve --dry-run` — preview options without recording decisions

## Workflow

### Step 1: Load Context

Run `{SCRIPT}` from the repository root and parse `FEATURE_DIR` from the JSON output. (If the `{SCRIPT}` placeholder was not substituted at install time, run `.specify/scripts/bash/check-prerequisites.sh --json --paths-only`, or on Windows `.specify/scripts/powershell/check-prerequisites.ps1 -Json -PathsOnly`.) Then load:

- **Required**: `ripple-report.md` (from a previous scan)
- **Required**: `tasks.md`, `spec.md`
- **Optional**: `plan.md`
- **Optional**: `blueprint.md` (produced by the spec-kit-blueprint companion extension, if installed)
- **Optional**: `ripple-fixes.md` (prior fix plans — their entries are carried over in Step 3f)

If `ripple-report.md` is missing, abort with: "Run `/speckit.ripple.scan` first."

### Step 2: Build Resolution Queue

1. Parse `ripple-report.md` and extract all findings with status `OPEN` or `WORSENED` — the statuses that need a fix plan. Include a `MITIGATED` finding only when the user names it by ID (to strengthen a partial fix). `RESOLUTION_PLANNED` (already planned), `RESOLVED`, `ACCEPTED_RISK`, and `STALE` findings are excluded unless named explicitly by ID.
2. Apply the user's filter if provided (severity or specific IDs)
3. Order the queue: CRITICAL → WARNING → INFO (within each severity, by finding ID)
4. Count total findings in scope

If no findings match the filter, report: "No open findings to resolve." and suggest running `/speckit.ripple.scan` if the report is outdated, or `/speckit.ripple.check` if fixes are awaiting verification.

Report the queue:

```
Found {N} findings to resolve: {count} CRITICAL, {count} WARNING, {count} INFO
Starting with R-{NNN}...
```

### Step 3: Sequential Resolution Loop

Present **EXACTLY ONE finding at a time**. Never reveal future findings in advance.

**`--dry-run` guard**: if `--dry-run` is set, execute only the sub-steps that read code or present output to the user; skip every sub-step that writes a file (currently 3d and 3f) and skip Step 4 entirely. Prefix each echoed decision with "(dry-run — not recorded)".

For each finding:

#### 3a: Present the Finding

Display a concise summary:

```markdown
## R-{NNN}: {Title} [{SEVERITY}]

**Category**: {category}

**What changed**: {Cause — the specific diff that introduced this}

**Before**: {behavior before the change}

**After**: {behavior after — and why it's a problem}

**Why tests miss it**: {explanation}
```

If the finding is `WORSENED` (a previous fix attempt backfired), also present the prior Resolution Strategy and what the check found — the new options must account for the failed attempt.

#### 3b: Analyze and Present Resolution Options

Read the affected code — locate the file and symbol from the finding's `Affected` anchor — and its dependents. Based on the actual codebase state, generate 2–4 resolution options. Each option must be:

- **Concrete** — specific enough to implement, not vague advice
- **Distinct** — each option represents a genuinely different strategy, not variations of the same fix
- **Honest about tradeoffs** — effort, risk, scope of change, what it does NOT solve

Present your **recommended option** prominently with reasoning, then show all options:

```markdown
**Recommended: Option {X}** — {1-2 sentence reasoning why this is the best balance of effort, safety, and correctness}

| Option | Strategy | Tradeoff |
|--------|----------|----------|
| A | {description} | {effort / risk / limitation} |
| B | {description} | {effort / risk / limitation} |
| C | {description} | {effort / risk / limitation} |
| Skip | Accept risk — document as known limitation | No code change; risk remains |

Reply with the option letter (e.g., "A"), accept the recommendation ("yes"), or describe your own approach.
```

**Option design rules:**
- Always include at least one **minimal fix** (smallest change that addresses the immediate risk)
- Always include at least one **structural fix** (addresses the root cause, may touch more files)
- Always include **Skip** — the user may intentionally accept the risk
- If the finding is INFO severity, bias toward lightweight options
- Never include options that require changes outside the project's control (e.g., "wait for library update")

#### 3c: Process the User's Response

| User says | Action |
|-----------|--------|
| Option letter ("A", "B", etc.) | Record that option as the chosen resolution |
| "yes" / "recommended" | Use the recommended option |
| "skip" | Mark as ACCEPTED_RISK with user's acknowledgment |
| Free-form text | Interpret as a custom resolution strategy — confirm understanding before recording |
| "stop" / "done" | End the loop early; remaining findings keep their current status |

If the response is ambiguous, ask for a quick clarification (does not count as a new finding).

#### 3d: Record the Resolution Decision

_Skipped in `--dry-run`._ After each accepted answer, immediately update `ripple-report.md`:

1. Update the finding's status:

```markdown
- **Status**: RESOLUTION_PLANNED
- **Resolution Strategy**: {Option X}: {description} — chosen on {date}
```

   For a finding that was `WORSENED`, the status also becomes `RESOLUTION_PLANNED`; reference the failed prior attempt in the strategy text.

2. If the user chose "Skip":

```markdown
- **Status**: ACCEPTED_RISK
- **Resolution Strategy**: Risk accepted — {user's reasoning if provided} ({date})
```

3. Save `ripple-report.md` after each decision (atomic write — don't batch).

#### 3e: Generate Implementation Guidance

After recording the decision, output a brief implementation note for the chosen strategy:

```markdown
### Implementation for R-{NNN}

**Files to modify**:
- `{path/to/file}` — {what to change}
- `{path/to/file}` — {what to change}

**Key steps**:
1. {step}
2. {step}
3. {step}

**Verification**: {how to confirm the fix works}
```

#### 3f: Refresh the Fix-Plan File

_Skipped in `--dry-run`._ After each recorded decision, regenerate `specs/{feature}/ripple-fixes.md` as a **derived view of the report**: one entry per finding that has a recorded plan and whose report status is `OPEN` or `RESOLUTION_PLANNED`. The file carries no status of its own — `ripple-report.md` is the sole status authority.

```markdown
# Ripple Fixes — regenerated {date}

> Derived from ripple-report.md — fix plans still awaiting application. Status lives in the report, not here.

## R-{NNN}: {Title} [{SEVERITY}]

**Strategy**: {Option X} — {description}

**Files to modify**:
- `{path/to/file}` — {what to change}

**Key steps**:
1. {step}
2. {step}

**Verification**: {how to confirm the fix works}

---
```

- Write the plan for the finding just decided, replacing any previous plan for the same `R-{NNN}`
- Carry over entries from the prior `ripple-fixes.md` (loaded in Step 1) for other findings whose status is still `OPEN` or `RESOLUTION_PLANNED`; drop all others — their plans are finished, failed, or accepted
- If no entries remain after regeneration, keep the header and write: "No fix plans awaiting application — see ripple-report.md for current finding status."
- After all resolutions, inform the user: "Fix plans saved to `specs/{feature}/ripple-fixes.md`. Run `/speckit.implement specs/{feature}/ripple-fixes.md` to apply them, or implement manually."

Then proceed to the next finding: "Next: R-{NNN} — {title} [{severity}]"

### Step 4: Update Report Summary

_Skipped entirely in `--dry-run`._ After all findings are processed (or the user stops early):

1. Update the `**Status**:` line in the ripple-report.md header to reflect new statuses. Do NOT alter the severity `**Findings**:` counts — they are cumulative.
2. **Update Resolution History** — append a row to the existing table, never create a duplicate section:

   - **If `## Resolution History` section already exists**: Find the existing table and append a new row at the bottom. Do NOT create a second `## Resolution History` heading.
   - **If `## Resolution History` section does not exist**: Create it once at the bottom of the report (before `## Check History` if that exists) with the table header and the first row.
   - Add a `### Session detail ({date})` sub-section below the table with per-finding option choices. If prior session details exist, leave them intact and append the new one after them.

```markdown
## Resolution History

| Date | Scope | Resolved | Accepted Risk | Skipped | Still Open |
|------|-------|----------|---------------|---------|------------|
| {prior rows unchanged} | | | | | |
| {datetime} | {all/critical/R-NNN} | {count} | {count} | {count} | {count} |
```

**Deduplication rule**: Before writing, scan the report for existing `## Resolution History` headings. If more than one exists (from a prior bug), merge all rows into a single table under one heading and remove the duplicate.

3. Update the Next Steps section to reflect remaining work

### Step 5: Report

Output a completion summary:

- Findings addressed in this session
- Decisions made: {N} resolution planned, {N} risk accepted, {N} still open
- If all CRITICAL findings have a resolution: "All critical findings have resolution strategies. Implement fixes, then run `/speckit.ripple.check` to verify."
- If CRITICAL findings remain OPEN: "{N} critical findings still need resolution."
- Suggest next step: implement the fixes, then `/speckit.ripple.check`
- In `--dry-run`, end with: "Dry run — no files were modified. Re-run without `--dry-run` to record decisions."

## Rules

- **One at a time**: Present exactly one finding per turn. Never batch findings or reveal the queue.
- **`--dry-run` is read-only**: In dry-run mode, never write `ripple-report.md` or `ripple-fixes.md` — present findings and options only.
- **Read before suggesting**: Always read the current state of the affected file before generating options. Options must be based on actual code, not the report's description alone.
- **Options must be implementable**: Every option (except Skip) must be specific enough that a developer (or `/speckit.implement`) can act on it without further design decisions.
- **Respect user decisions**: If the user chooses an option you didn't recommend, do not argue. Record it and move on.
- **No auto-fixing**: This command records decisions and provides guidance. It does NOT modify source code. Code changes happen in a separate implementation step.
- **Atomic saves**: Write `ripple-report.md` after each decision, not at the end. This prevents loss if the session is interrupted.
- **Severity-appropriate depth**: CRITICAL findings get detailed multi-option analysis. INFO findings can have simpler 2-option choices (fix / skip).
- **Never exceed the report**: Only resolve findings that exist in `ripple-report.md`. Do not surface new issues — that's what `/speckit.ripple.scan` is for.
- **Language of the session**: Follow the language used in the existing `ripple-report.md`.
