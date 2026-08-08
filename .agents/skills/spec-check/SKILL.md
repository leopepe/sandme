---
name: spec-check
description: Use before implementation begins to verify a spec exists, is at Status Accepted, has no open questions, and is not contradicted by existing Accepted or Implemented specs.
---

# Spec check

Validate that a spec is ready for implementation **before** any code is written. This is a
pre-flight check, not a full audit — it catches the common mistakes that waste implementation
time: missing specs, unapproved specs, open questions, and contradictions with existing specs.

Run this skill at the start of the Apply stage (`spec-driven-development.md` §3.3), before
writing any code. If any check fails, stop and fix the spec first.

## Not this skill's job

| Question | Skill that owns it |
| --- | --- |
| Is the spec complete and well-formed? | `spec-generation` — it writes specs; this skill checks readiness |
| Is the spec implemented correctly? | `spec-review` — it audits specs against code after implementation |
| Does the code follow guidelines? | `review-standards` — this skill checks spec readiness, not code quality |

## 1. Check that a spec exists

Search `docs/specs/` for a spec that covers the capability being implemented:

```
ls docs/specs/*.md
```

Read every spec except `0000-spec-template.md`. Look for:

- A spec whose **Summary** or **Problem** describes the capability.
- Requirements (`FR-`/`NFR-`) that cover the behaviour being implemented.

**If no spec exists:** Stop. Report: "No spec found for this capability. Write a spec first using
the `spec-generation` skill or `spec-generator` agent." Do not implement.

**If multiple specs cover the capability:** Report all of them and ask the user which one to
implement. Do not guess.

## 2. Check the spec's Status

Read the spec's **Metadata** section and check the `Status` field.

- **`Draft` or `Review`:** The spec is not approved. Stop. Report: "Spec is at Status:
  <Draft|Review>. The user must approve it before implementation begins. Set Status: Accepted
  after resolving open questions."
- **`Accepted`:** The spec is approved and ready to implement. Continue.
- **`Implemented`:** The capability is already implemented. Stop. Report: "Spec is at Status:
  Implemented. The capability already exists. Write a new spec with deltas if you are changing
  it (see `spec-driven-development.md` §4)."
- **`Superseded`:** The spec has been replaced. Stop. Report: "Spec is superseded by
  SPEC-NNNN. Implement the replacement spec instead."

Only `Accepted` specs may be implemented.

## 3. Check that open questions are resolved

Read the spec's **Open questions** section.

- If the section is non-empty (any `[NEEDS CLARIFICATION: ...]` markers remain), stop. Report:
  "Open questions are not resolved. Resolve them before implementation begins. See
  `spec-driven-development.md` §3.3."
- If the section is empty, continue.

Open questions MUST be empty before `Status` leaves `Draft`. If a spec is at `Accepted` with
open questions, it is a bug — the status was advanced prematurely.

## 4. Check for contradictions with existing specs

Read every other spec at `Status: Accepted` or `Implemented`. Check whether the spec being
implemented contradicts any of them:

- Do two specs define the same requirement (`FR-`/`NFR-`) with different behaviour?
- Does the spec being implemented change behaviour that an `Implemented` spec defines, without
  a **Spec deltas** section?
- Do two specs use the same requirement ID (`FR-001` in two different specs)?

**If a contradiction is found:** Stop. Report the contradiction with the spec IDs and requirement
IDs involved. Ask the user to resolve it before implementation begins. See
`spec-driven-development.md` §4 for how to write deltas.

**If no contradictions are found:** Continue.

## 5. Check that related ADRs are accepted

Read the spec's **Related ADRs** field. For each ADR listed:

- Check its `Status` field.
- If any ADR is at `Status: Proposed`, stop. Report: "ADR-NNNN is at Status: Proposed. ADRs
  must be at Status: Accepted before implementation begins. See
  `spec-driven-development.md` §2.1."
- If all ADRs are at `Status: Accepted`, continue.

Implementation must not begin while its architectural decisions are still being discussed.

## 6. Produce the report

Emit exactly these sections, in this order:

```markdown
## Spec check

**Spec:** SPEC-NNNN — <title>
**Status:** <Accepted | Draft | Review | Implemented | Superseded>
**Open questions:** <count, or "none">
**Related ADRs:** <list with statuses, or "none">
**Contradictions:** <list, or "none">

### Result
<Ready to implement, or what must be fixed first>
```

If all checks pass, the result is: "Ready to implement. Proceed with the Apply stage
(`spec-driven-development.md` §3.3)."

If any check fails, the result is: "Not ready. Fix <issue> before implementation begins."

## 7. Stop

Report only. Do not edit the spec, do not write code, do not resolve open questions. Those are
separate, requested steps that follow `spec-driven-development.md` §3.3.

## Red flags

| If you catch yourself... | Do this instead |
| --- | --- |
| Implementing a `Draft` or `Review` spec | Stop. The spec must be at `Accepted` |
| Implementing with open questions | Stop. Resolve them first |
| Ignoring a contradiction with an existing spec | Stop. Resolve it before implementing |
| Implementing a `Superseded` spec | Stop. Implement the replacement instead |
| Implementing without a spec | Stop. Write a spec first |
| Skipping the ADR status check | Check every related ADR; `Proposed` ADRs do not authorize implementation |
| Editing the spec to make it pass | Report the issue; fixes are a separate step |
