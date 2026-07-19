---
description: Read a design spec and produce an implementation plan with TDD tasks, validated by plan-validator
argument-hint: "[design-spec-file]"
---

# feature-plan — Plan Phase Orchestrator

You are orchestrating the **plan phase**. Your job is to take an approved design spec, run `superpowers:writing-plans` to produce an implementation plan, validate it with `plan-validator`, and present it for final approval before handing off to `/feature-dev`.

The design spec file is: `$ARGUMENTS`

If `$ARGUMENTS` is empty, list available specs from `docs/superpowers/specs/` and ask the user to choose.

---

## Task Template Specification

Every task in the implementation plan MUST use this schema. This is the **single source of truth** — `feature-design` references it when describing plan output, and `feature-dev` validates against it at intake.

```yaml
- id: T1                          # Sequential ID, ordered by dependency
  title: "<imperative phrase>"    # e.g. "Add clipboard paste handler"
  files:                          # ≤3 files per task (enforced)
    - path/to/file.rs             # Files to CREATE or MODIFY
    - path/to/file_test.rs        # Test file always listed
  depends_on: []                  # Task IDs this task requires (e.g. [T0])
  breaking: false                 # true if API/DB/storage/crate boundary changes
  # --- Acceptance Criteria (GIVEN/WHEN/THEN) ---
  ac:
    - given: "<precondition>"
      when: "<action>"
      then: "<observable result>"   # Machine-verifiable: must be testable
  # --- Regression Guard ---
  regression:
    - "cargo test <existing_test> -- --exact"  # Tests that MUST keep passing
  # --- Migration Guard (only if task involves DB migration) ---
  migration_verify:
    - "<step to verify migration>"  # e.g. "Check new column exists: SELECT ..."
  # --- Implementation (TDD ordering enforced — test_code ALWAYS before impl_code) ---
  test_code: |
    // Test code — written FIRST, verified to fail, then implementation follows
  impl_code: |
    // Implementation — MINIMAL code to make the test pass
  # --- Verification ---
  verify:
    - "cargo test <test_name> -- --exact"  # Must pass
    - "cargo clippy --workspace -- -D warnings"
```

### Field Rules

| Field | Constraint | Reject if |
|-------|-----------|-----------|
| `id` | Sequential T0, T1, T2... | Gaps or duplicates |
| `files` | **≤3** files, test file always included | >3 files (split the task) |
| `depends_on` | Explicit IDs, empty `[]` if independent | Missing dependency declaration |
| `breaking` | `true` for API/DB/storage/crate boundary changes | Breaking change not marked |
| `ac` | ≥1 GIVEN/WHEN/THEN triple, each `then` observable | Missing AC, or `then` is vague ("works correctly") |
| `regression` | ≥1 existing test that must keep passing | Empty regression list |
| `migration_verify` | Required if task touches `migrations/` | Migration without verify steps |
| `test_code` | Non-empty, MUST appear before `impl_code` | `impl_code` written before `test_code` |
| `verify` | ≥1 concrete command | Missing verification |

---

## Pipeline Phases

### Phase 0: Intake

1. Read the design spec at `$ARGUMENTS`. If the file does not exist, abort: "No design spec found. Run `/feature-design <feature description>` first."
2. Read `.omo/summary/summary_and_next.md` for current project state and version.
3. Confirm the spec is a completed design doc (not a work-in-progress). If it has TODO markers or incomplete sections, warn the user.
4. Restate the feature: what it does, key design decisions from the spec.

### Phase 1: Writing Plans

Invoke `superpowers:writing-plans` skill with the design spec.

**Task format**: Every task MUST conform to the **Task Template Specification above**:
- `id`: sequential T0, T1, T2...
- `files`: ≤3 files per task, test file always included
- `depends_on`: explicit task IDs for dependency ordering
- `breaking`: true/false for API/DB/storage boundary changes
- `ac`: ≥1 GIVEN/WHEN/THEN triple with observable, machine-verifiable `then`
- `regression`: ≥1 existing test that must keep passing
- `migration_verify`: required if task touches migrations or models
- `test_code` before `impl_code`: TDD order enforced — test first
- `verify`: ≥1 concrete command

**No placeholders**: no TBD, no "similar to task N", no "implement later".
**Order**: tasks ordered by dependency (Level 0 → Level 1 → Level 2 → ...).

Output: `docs/superpowers/plans/YYYY-MM-DD-<feature-name>.md`

**The plan header must include** an "Agent Worker Instructions" section:
- Required sub-skills for execution
- Recommended execution mode: `subagent-driven-development` (preferred) or `executing-plans`
- Required verification: `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`
- Version bump reminder

### Phase 2: Validate

After the plan is written, but before presenting to the user:

1. **Run plan-validator** — dispatch the `plan-validator` agent with the plan file:
   ```
   task(subagent_type="plan-validator", run_in_background=false,
        prompt="docs/superpowers/plans/<plan-file>.md")
   ```

2. **If verdict is FAIL** (blocking issues found):
   - Show BLOCKING findings to the user.
   - Fix each BLOCKING issue in the plan file.
   - Re-run validation until verdict is PASS or PASS WITH ADVISORY.
   - Do NOT present the plan while blocking issues exist.

3. **If verdict is PASS WITH ADVISORY** (no blocking, advisory only):
   - Include ADVISORY findings in the presentation so the user is aware.
   - Proceed to gate.

### Phase 3: Gate

Present the validated plan:

1. Summary: files to create/modify, total tasks, key decisions, estimated effort.
2. Include validation report (all 9 dimensions, any advisories).
3. **GATE**: Ask the user to review the plan. Do NOT proceed to implementation.
4. Tell the user: "Plan is ready. Run `/feature-dev docs/superpowers/plans/<plan-file>.md` to execute."

---

## Project-Specific Rules (from AGENTS.md)

- Plan language: task titles and AC in English (code-level), plan header/context in Chinese if from a Chinese spec.
- Verification gates: `cargo clippy --workspace -- -D warnings` + `cargo test --workspace` + `npm run build`.
- Version bump: patch for fixes, minor for features.
- Rust: functions ≤50 lines, lines ≤120 chars.
- DB: migrations auto-apply, run-time queries only (no `query!` macro).
