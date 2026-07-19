---
description: Identify the next pending feature and orchestrate design→plan pipeline for it
---

# feature-next — Next Feature Entry Point

You are the **feature entry point**. Your job is to read the project's current state, identify the next feature to implement from the pending list, present it to the user, and on confirmation orchestrate the sequential pipeline: `/feature-design` → `/feature-plan` → (user runs `/feature-dev`).

---

## Pipeline Phases

### Phase 0: Load Project State

Read these files in parallel:

1. `.omo/summary/summary_and_next.md` — extract:
   - The "## 待实施" (Pending) table: phase, title, dependencies for each row.
   - The "下一步" (Next Step) hint if present.
   - The most recent completed phase (marked ✅).

2. `docs/superpowers/specs/` — find the **latest** design spec doc (by filename date, most recent first):
   ```bash
   ls -t docs/superpowers/specs/*.md | head -1
   ```
   Read this file and extract the section matching the next feature's phase code.

### Phase 1: Identify Next Feature

Algorithm for selecting the next feature:

```
FOR each row in "待实施" table, in order:
  IF all dependency phases are marked ✅ in completed sections:
    → This is the next feature. STOP.
```

**Tie-breaking**: The "下一步" hint in summary_and_next.md takes priority if it names a specific phase. Otherwise, use table order.

### Phase 2: Present to User

Present the finding with context:

```
## Next Feature: {phase} — {title}

**From**: {latest-spec-doc-filename}
**Dependencies**: {dependency list with ✅ status}
**Summary**: {2-3 sentence summary from the spec doc}
**Estimated effort**: {from spec doc if available}

### Options:
1. **Proceed** — Run `/feature-design` to produce a design spec, then `/feature-plan` to produce an implementation plan. Sequential with gates between each.
2. **Pick a different feature** — Show the full "待实施" table and let me choose.
3. **Skip** — Do nothing.
```

Use `AskUserQuestion` to present these 3 options. Do NOT proceed until the user chooses.

### Phase 3: Orchestrate Design → Plan

If the user chooses option 1 (Proceed), run two sequential phases with a gate between them.

#### Phase 3a: Design

Extract the feature's description from the latest spec doc, then invoke the `/feature-design` pipeline:

```
Feature: {phase} — {title}
Source: {spec-doc-path}, section {section-name}

Description: {full feature description from spec doc, including all sub-sections}

Context from summary_and_next.md:
- Dependencies: {list}
- Related completed features: {list}
```

This produces a design spec at `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`.

**GATE**: After the spec is approved, do NOT proceed automatically. Ask the user: "Design spec is complete. Run `/feature-plan` to produce an implementation plan, or skip?"

#### Phase 3b: Plan

If the user confirms, invoke the `/feature-plan` pipeline with the newly created spec file as input.

This produces an implementation plan at `docs/superpowers/plans/YYYY-MM-DD-<feature-name>.md`, automatically validated by `plan-validator` (see `/feature-plan` for the Task Template Specification and validation dimensions).

**GATE**: After the plan is approved, tell the user: "Implementation plan is ready. Run `/feature-dev docs/superpowers/plans/<plan-file>.md` to execute."

---

## Feature-Specific Context

When the spec doc mentions the feature in multiple sections (e.g., overview table + detail section + gantt chart), gather ALL relevant context:

- **Overview table**: phase code, title, priority, dependencies, estimated days
- **Detail section**: full functional requirements, UI mockups, API specs, DB changes
- **Risk table**: any security concerns (SSRF, etc.), mitigation steps
- **Gantt/dependency graph**: where this fits in the overall timeline

Feed this complete context into the brainstorming phase so the design doesn't miss constraints already documented.

---

## Edge Cases

**If "待实施" table is empty**: "All features are implemented. No pending features found. Check summary_and_next.md — if the table should be non-empty, update it first."

**If the next feature has unmet dependencies**: List what's blocking it. "P4-B requires P4-A (✅ done). However, P4-C has no dependencies and can proceed immediately. Suggest P4-C instead?"

**If the latest spec doc doesn't have a section for the next feature**: "The spec doc {name} doesn't contain a section for {phase}. Check if there's an older spec doc that covers it, or if the feature needs a new spec."

**If no spec docs exist at all**: "No design specs found in docs/superpowers/specs/. Run /feature-design with a feature description to create the first spec."

**If summary_and_next.md is missing or corrupt**: "Cannot determine project state. Read AGENTS.md and docs/superpowers/specs/ manually to assess pending work."

---

## Project-Specific Rules (from AGENTS.md)

- Design specs: in Chinese, use Mermaid or UML diagrams.
- Plan tasks: TDD-first (test code before implementation), exact file paths, no placeholders.
- Verification gates: `cargo clippy --workspace -- -D warnings` + `cargo test --workspace` + `npm run build`.
- Version bump: patch for fixes, minor for features.