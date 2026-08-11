# Ripple

> Detect side effects that tests can't catch after implementation.

![Spec Kit >= 0.2.0](https://img.shields.io/badge/spec--kit-%3E%3D0.2.0-blue)
![Version 1.1.0](https://img.shields.io/badge/version-1.1.0-green)
![License MIT](https://img.shields.io/badge/license-MIT-brightgreen)

A [Spec Kit](https://github.com/github/spec-kit) community extension that analyzes your implementation for hidden ripple effects — the kind of side effects that pass all tests but break things in production.

## The Problem

Tests verify **intended behavior**. But every code change creates ripple effects that tests aren't designed to catch:

- A function signature looks the same, but its return value *means* something different now
- A new dependency introduces a subtle ordering constraint
- A refactored module now holds onto resources longer than before
- An async operation that used to complete in time no longer does under load

These issues hide in the gap between "all tests pass" and "production is stable."

## What Ripple Does

Ripple is **not a general code review**. It compares your implementation against the baseline (branch point) and asks: *"What did this change break or put at risk that wasn't broken before?"*

Every finding is **delta-anchored** — causally linked to a specific change in your diff, with a clear before/after description. Pre-existing issues are out of scope.

It analyzes across **9 categories**:

| Category | What It Catches |
|----------|----------------|
| **Data Flow** | Input/output shape mismatches, silent data loss, serialization gaps |
| **State & Lifecycle** | Global state pollution, resource leaks, initialization order issues |
| **Interface Contract** | Semantic signature changes, broken implicit contracts |
| **Resource & Performance** | Complexity regressions, hot-path allocations, I/O amplification |
| **Concurrency** | Race conditions, lock ordering, atomicity assumptions |
| **Distributed Coordination** | Idempotency gaps, ordering assumptions, partition tolerance |
| **Configuration & Environment** | Missing config keys, environment-specific gaps, deploy ordering |
| **Error Propagation** | Unhandled failure modes, silent swallowing, partial failure states |
| **Observability** | Lost trace context, logging gaps, broken metrics |

Categories are **domain-agnostic** — they apply whether you're building a web API, CLI tool, mobile app, embedded system, data pipeline, or anything else. Security concerns (access control bypass, sensitive data exposure, privilege escalation) are covered as a cross-cutting lens within relevant categories rather than as a separate category. Domain-specific details are inferred from the actual codebase — Ripple adapts its analysis to the project's technology stack automatically.

## Example Finding

Each finding traces a causal chain from the change to its impact:

```markdown
#### R-001: Config file allows 20MB uploads but controller rejects above 10MB

- **Category**: Configuration & Environment
- **Cause**: `application.yml` max-file-size raised to 20MB, but controller
  still has `MAX_IMAGE_SIZE = 10MB` hardcoded
- **Affected**: `src/controllers/UploadController.java` › `validateUpload()` (≈line 42)
- **Blast Radius**: any client told 20MB is now allowed; API docs generated
  from config
- **Before**: Both limits were 10MB — consistent, uploads above 10MB rejected
- **After**: Multipart accepts 20MB but controller immediately rejects >10MB
  with a 400 error. The config change has no effect.
- **Why Tests Miss It**: Tests use small fixtures; boundary tests at 10-20MB
  range don't exist
- **Recommendation**: Unify to a single config source
- **Status**: OPEN
```

## Fix-Induced Detection

Fixes can create new problems. Ripple tracks this by re-scanning after each fix cycle:

```
1st scan:  3 critical, 5 warning, 3 info    → fix all
2nd scan:  1 critical, 4 warning, 6 info    → new issues from fixes
3rd scan:  0 critical, 1 warning, 4 info    → converging
```

This loop continues until findings stabilize. The `check` command explicitly scans for side effects introduced by the fixes themselves.

## Installation

```bash
specify extension add ripple --from https://github.com/chordpli/spec-kit-ripple/archive/refs/tags/v1.1.0.zip
```

Once Ripple is listed in the Spec Kit community catalog:

```bash
specify extension add ripple
```

## Commands

### `/speckit.ripple.scan`

Analyze implementation for untested side effects.

```bash
/speckit.ripple.scan                 # Full scan, all severities
/speckit.ripple.scan critical        # Critical findings only
/speckit.ripple.scan --diff          # Incremental scan on changed files
/speckit.ripple.scan --base develop  # Feature was branched off develop
```

The baseline is detected automatically (remote default branch, then `origin/main`/`origin/master`/`origin/develop`, then local `main`/`master`); use `--base <ref>` to override. The scan diffs the **working tree** against the baseline, so uncommitted implementation changes are included.

**Produces**: `specs/{feature}/ripple-report.md`

### `/speckit.ripple.resolve`

Interactively walk through findings and decide how to fix each one.

```bash
/speckit.ripple.resolve             # Resolve open + worsened findings, CRITICAL first
/speckit.ripple.resolve critical    # Resolve critical findings only
/speckit.ripple.resolve R-001 R-003 # Resolve specific findings
/speckit.ripple.resolve --dry-run   # Preview options without recording decisions
```

For each finding, Ripple presents:
1. The cause (what changed) and the side effect (what's at risk)
2. 2-4 concrete resolution options with tradeoffs (minimal fix, structural fix, skip)
3. A recommended option with reasoning

You pick an option, describe your own approach, or skip. Decisions are recorded in `ripple-report.md`, and fix plans are saved to `specs/{feature}/ripple-fixes.md` — a derived view of the report holding only plans still awaiting application. Point `/speckit.implement` at that file to apply them, or implement manually.

### `/speckit.ripple.check`

Re-verify findings after fixes have been applied.

```bash
/speckit.ripple.check             # Re-check all non-terminal findings
/speckit.ripple.check critical    # Re-check critical findings only
/speckit.ripple.check R-001 R-005 # Re-check specific findings
```

**Updates**: existing `ripple-report.md` with resolution status

Crucially, check also detects **fix-induced side effects** — new problems created by the fixes themselves. If a fix introduced a new risk, check will catch it and add it as a new finding.

check ends with a **convergence verdict**: CONVERGED when no findings await action, NOT CONVERGED with a breakdown otherwise — so you know when the loop is done, independent of the critical-path message.

## Hook

Ripple hooks into `after_implement` — after running `/speckit.implement`, you'll be prompted:

```
Scan for untested side effects? (y/n)
```

The hook fires after *every* `/speckit.implement` run — including the one that applies Ripple fix plans. At that point in the loop the right command is `/speckit.ripple.check` (it alone transitions planned findings and runs fix-induced analysis), so run it explicitly; answering "y" to the scan prompt won't verify your fixes.

## Workflow Position

```
/speckit.specify                        → spec.md
/speckit.clarify                        → clarifications
/speckit.plan                           → plan.md
/speckit.tasks                          → tasks.md
/speckit.implement                      → code
                                          ↓ after_implement hook (optional)
┌──  /speckit.ripple.scan               → ripple-report.md  ★
│    /speckit.ripple.resolve            → ripple-fixes.md   ★
│    Implement fixes
│    /speckit.ripple.check              → updated report    ★
│              ↓
└── New findings? → loop back
                                          ↓ No
/speckit.checklist                      → verify
Merge with confidence
```

Spec Kit's core workflow goes from `/speckit.implement` directly to merge. This extension inserts a feedback loop after implementation: scan for side effects, decide how to fix them, verify the fixes, and repeat until findings stabilize.

## Artifacts

| File | Created by | Purpose |
|------|-----------|---------|
| `specs/{feature}/ripple-report.md` | `scan`, updated by `resolve` and `check` | Side effect findings, resolution history, check history |
| `specs/{feature}/ripple-fixes.md` | `resolve`, refreshed by `check` | Fix plans still awaiting application — a derived view of the report (status lives only in `ripple-report.md`). Point `/speckit.implement` at it, or implement manually |

## Severity Levels

| Level | Meaning |
|-------|---------|
| **CRITICAL** | Could cause data loss, security breach, or system outage in production |
| **WARNING** | Likely to cause bugs, degraded performance, or operational issues |
| **INFO** | Potential concern worth reviewing — may be intentional or low-risk |

## Complementary Perspectives

Three post-implementation extensions share the `after_implement` hook. Each addresses a distinct concern:

| | [`review`](https://github.com/ismaelJimenez/spec-kit-review) | [`staff-review`](https://github.com/arunt14/spec-kit-staff-review) | `ripple` |
|---|---|---|---|
| **Focus** | Code quality | Shipping readiness | Change impact |
| **Analyzes** | Changed code (quality, tests, types, error handling) | Changed code vs. spec (security, performance, coverage) | Unmodified code affected by changes |
| **Finding depth** | Concise | Concise with verdict | Detailed — causation, before/after, blast radius |
| **Catches uniquely** | Design flaws, code style, pre-existing vulnerabilities, process gaps | Spec adherence gaps, overall test absence, ship/hold decision | Fix-induced regressions, causal chains across changes, implicit contract shifts |

When run on the same change set, findings fall into three groups:

- **Overlap** — all tools catch the issue, but from different angles. Review flags it concisely; Ripple traces the causal chain (which change introduced it, what it looked like before, what's at risk now).
- **Review/staff-review only** — pre-existing risks, design quality, validation logic errors, overall test absence, process gaps. Outside Ripple's delta-anchored scope by design.
- **Ripple only** — fix-induced regressions that emerge from the *interaction* between changes, not from any single file. Review tools tend to miss these because they require tracing cause-and-effect across the diff.

**Strongest when combined**: Review catches what's wrong with your change. Ripple catches what your change did to everything else. Running only one leaves blind spots.

## Usage Tips

| PR Size | Recommendation |
|---------|---------------|
| Small (1-5 files) | `/speckit.ripple.scan` — full scan |
| Medium (6-15 files) | `/speckit.ripple.scan` — full scan, consider `critical` filter if findings are noisy |
| Large (16+ files) | `/speckit.ripple.scan critical` — start with critical only, then expand if needed |
| After fixes | `/speckit.ripple.check` — it alone verifies findings, detects WORSENED, and runs fix-induced analysis |
| After further changes | `/speckit.ripple.scan --diff` — incremental scan to find *new* side effects in files changed since the last scan |

Ripple reads the full diff plus blast radius files. Larger change sets consume more context. Use filters to keep scans focused.

## Troubleshooting

| Error | Solution |
|-------|----------|
| "Run `/speckit.tasks` first" | Generate tasks before running scan |
| "Run `/speckit.ripple.scan` first" | Scan must run before resolve or check |
| No findings generated | Confirm the implementation exists on disk and there are committed/staged/unstaged/untracked changes vs. the base branch; if your default branch isn't `main`/`master`/`develop`, pass `--base <ref>` |
| Command not available | Check `specify extension list`, restart agent session, reinstall |

## Requirements

- Spec Kit >= 0.2.0
- Existing spec artifacts (`spec.md`, `plan.md`, `tasks.md`)
- Implemented code on disk

## License

MIT — see [LICENSE](LICENSE)

## Support

- **Issues**: <https://github.com/chordpli/spec-kit-ripple/issues>
- **Spec Kit**: <https://github.com/github/spec-kit>

---

*Extension Version: 1.1.0 | Spec Kit: >=0.2.0*