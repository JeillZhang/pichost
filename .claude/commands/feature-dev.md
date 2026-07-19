---
description: Execute a feature implementation plan with superpowers worktree→subagent-dev→review→finish pipeline
argument-hint: "[plan-file-path]"
---

# feature-dev — Implementation Phase Orchestrator

You are orchestrating the **implementation phase** of a feature. Your job is to take a completed implementation plan and execute it through the full superpowers pipeline: isolated workspace → subagent-driven TDD → code review → system integration testing → finish.

**TDD is NON-NEGOTIABLE.** Every task must follow RED→GREEN→REFACTOR. If an implementer subagent produces implementation code before test code, reject it and re-dispatch.

The plan file is: `$ARGUMENTS`

If `$ARGUMENTS` is empty, ask the user which plan to execute. List available plans from `docs/superpowers/plans/`.

**Plan format**: Every task in the plan MUST conform to the **Task Template Specification** defined in `/feature-plan`. Phase 0 validates against it.

---

## Pipeline Phases

### Phase 0: Intake & Validate

1. Read the plan file at `$ARGUMENTS`. If the file does not exist, abort and ask the user to run `/feature-plan` first.
2. Read `.omo/summary/summary_and_next.md` for current project state.
3. **Validate the plan against the Task Template Specification (see `/feature-plan`):**
   - Every task has all required fields (`id`, `files`, `ac`, `regression`, `test_code`, `impl_code`, `verify`).
   - No task has >3 `files` entries. If it does, reject: "Task {id} has {N} files. Split into smaller tasks (max 3)."
   - No task has `impl_code` appearing before `test_code` in the plan. Reject: "Task {id} lists implementation before test. TDD order must be: test first."
   - All `depends_on` values reference existing task IDs. Reject circular or dangling dependencies.
   - Any task touching `migrations/` or `pichost-core/src/models.rs` has `migration_verify` steps.
   - Every `ac.then` is observable: it describes a specific output, state change, HTTP status, or CLI exit code. Reject vague AC: "Task {id} AC 'then: {text}' is not observable. Rewrite as a concrete assertion."
   - Every task has ≥1 `regression` entry naming an existing test. If no existing tests cover the area: `regression: ["cargo test --workspace (full suite — no existing tests for this module)"]`.
   - Note: full quality validation (AC observability, dependency graph, migration safety, etc.) is done by `plan-validator` in `/feature-plan` Phase 2 — do NOT re-run it here.
4. Restate the plan scope: what will be built, files to touch, estimated effort.
5. Classify the feature size:
   - **trivial**: 1 task, 1 file → inline execution OK
   - **standard**: 2-5 tasks → subagent-driven (1 subagent per task, sequential)
   - **large**: 5+ tasks, cross-cutting → subagent-driven with parallel independent tasks

### Phase 1: Isolated Workspace

Invoke `superpowers:using-git-worktrees` to create an isolated workspace.

**Key rules:**
- Branch name: `feat/<plan-name>` (from the plan file, kebab-case)
- Detect existing isolation first — if already in a worktree, skip creation.
- Prefer native workspace tools. Fall back to `git worktree add`.
- Verify clean test baseline: `cargo test --workspace` and `cargo clippy --workspace -- -D warnings` must pass before starting.
- If tests fail on baseline, abort and report — do NOT proceed with implementation.

### Phase 2: Execute — Subagent-Driven Development

Invoke `superpowers:subagent-driven-development` skill.

**The plan file drives everything.** For each task in the plan:

1. **Generate task brief** — extract all template fields into a self-contained brief:

   ```
   TASK: {id} — {title}
   FILES: {files list}
   DEPENDS_ON: {depends_on} (T0, T1 already completed)
   BREAKING: {breaking} — if true, reviewer MUST flag this task for extra scrutiny

   ACCEPTANCE CRITERIA:
     GIVEN {ac.given}
     WHEN {ac.when}
     THEN {ac.then}

   REGRESSION GUARD: {regression commands — must keep passing}

   MIGRATION VERIFY (if applicable): {migration_verify steps}

   TDD ORDER:
     1. FIRST: Write test code → {test_code}
     2. Verify test FAILS → cargo test {test_name} -- --exact (expect FAIL)
     3. SECOND: Write minimal implementation → {impl_code}
     4. Verify test PASSES → cargo test {test_name} -- --exact (expect PASS)

   FINAL VERIFY:
     {verify commands}
   ```

2. **Dispatch implementer subagent** — a fresh `deep` or `unspecified-high` agent with the task brief as its sole prompt. The brief MUST include the `TDD ORDER` block so the subagent knows it cannot write implementation before test code.
3. **Implementer reports**: `done` / `needs-more-context` / `blocked` / `done-with-concerns`.
4. **Dispatch reviewer subagent** — review against both the spec (does it meet requirements?) and code quality (does it follow project conventions?).
5. **Fix cycle** — if reviewer finds issues, dispatch a fix subagent. Repeat until clean.
6. **Mark task complete**.

**Verification after every task:**
- `cargo clippy --workspace -- -D warnings` must be zero.
- `cargo test --workspace` must pass (except pre-existing ignored tests).
- If the task touches frontend: `npm run build` must pass.

**Persistence:** Track progress in `.superpowers/sdd/progress.md`. If the session is interrupted, resume from the last completed task.

**Key rules from subagent-driven-development:**
- Sequential execution — do NOT fan out multiple implementers for interdependent tasks.
- Fresh subagent per task — never reuse a subagent across tasks (context pollution).
- Only fan out implementers in parallel for tasks marked as independent in the plan.
- Task briefs must be self-contained — the subagent should need zero additional context.

**TDD Protocol (RED→GREEN→REFACTOR):**

For EVERY task in the plan, enforce this cycle. Any deviation is a blocking failure.

1. **RED** — Write the failing test FIRST.
   - The implementer subagent MUST produce test code before any implementation code.
   - Verify the test fails: `cargo test <test_name> -- --exact` must return non-zero.
   - If the test passes without implementation (false green), reject the output. The test must prove the feature is absent.
   - For frontend-only tasks: write a test that verifies the component renders or the hook returns expected initial state, and verify it fails first.

2. **GREEN** — Write minimal implementation to pass the test.
   - The implementer subagent must write ONLY the code that makes the failing test pass.
   - No extras, no "future-proofing", no unrelated refactoring.
   - Verify: `cargo test <test_name> -- --exact` must return zero.
   - Verify: `cargo test --workspace` must pass (no regressions).

3. **REFACTOR** — Clean up while keeping tests green.
   - Apply `rust-refactor-fns` skill if any function exceeds 50 lines or any line exceeds 120 chars.
   - Remove duplication, improve names, extract helpers — but change NO behavior.
   - Verify: `cargo test --workspace` must still pass.
   - Verify: `cargo clippy --workspace -- -D warnings` must be zero.

**Enforcement rules for the orchestrator (YOU):**
- Before dispatching any implementer subagent, confirm the task brief includes explicit test code.
- If the subagent returns implementation without test code → **REJECT**. Re-dispatch with the brief: "Write the test first. Do NOT write implementation code until the test exists and fails."
- After the subagent returns, personally verify the test was written first by checking the commit/diff order.
- If a task in the plan does NOT specify test code, pause and ask the user to update the plan before continuing.

### Phase 3: Full-Branch Code Review

After all tasks complete:

1. Invoke `superpowers:requesting-code-review` skill.
2. Provide: feature description, base SHA, head SHA, plan file path.
3. Dispatch the `ecc:code-reviewer` agent (or use the built-in code-review flow).
4. Also dispatch `ecc:rust-reviewer` for any `.rs` file changes.
5. If the diff touches auth, crypto, storage, or DB — also dispatch `ecc:security-reviewer`.
6. **Review findings handling:**
   - **CRITICAL** — fix immediately, re-verify, re-review.
   - **HIGH** — fix before proceeding to Phase 4 (Integration Testing).
   - **MEDIUM** — document, fix if time permits.
   - **LOW** — acknowledge, optional fix.

### Phase 4: System Integration Testing

After code review passes, verify the feature works in a real deployment environment. This phase gates the finish — if integration tests fail, do NOT proceed to Phase 5.

#### Step 1: Bring up full stack

```bash
# Build and start the full stack (API, worker, PostgreSQL, Redis, Nginx)
docker compose up --build -d

# Wait for services to be healthy. API migrations auto-apply on startup.
# Poll health endpoint until ready (max 30s)
for i in $(seq 1 30); do
  curl -s http://localhost/api/health && break
  sleep 1
done
```

#### Step 2: Run integration test suite

The integration tests in `pichost-api/tests/` require a live database and Redis. Docker Compose provides these.

```bash
# Set connection vars for integration tests and run the full suite
DATABASE_URL=postgres://pichost:pichost@localhost:5432/pichost \
PICHOST_DATABASE_URL=postgres://pichost:pichost@localhost:5432/pichost \
PICHOST_REDIS_URL=redis://localhost:6379 \
PICHOST_AUTH_JWT_SECRET=test-integration-secret-at-least-32-chars \
  cargo test --workspace
```

**All tests must pass**, including the previously-ignored integration tests:
- `pichost-api/tests/gallery_test.rs` (4 tests)
- `pichost-api/tests/health_test.rs` (1 test)
- `pichost-api/tests/admin_test.rs` (6 tests)

#### Step 3: API smoke test

Verify basic endpoints respond correctly in the running deployment:

```bash
# Health check returns 200
curl -sf http://localhost/api/health

# Metrics endpoint returns Prometheus data
curl -sf http://localhost/metrics | grep -q "pichost"

# Public image serving returns 404 for non-existent (expected — proves routing works)
test "$(curl -s -o /dev/null -w '%{http_code}' http://localhost/u/nonexistent)" = "404"
```

#### Step 4: Browser E2E Smoke Test (if web-ui/ changed)

If the feature touches frontend code, run an automated browser test to verify UI pages load, render, and navigate without errors. This uses Playwright to drive a real Chrome instance.

```bash
# Install Playwright if not already available
cd web-ui
npx playwright install chromium 2>&1 | tail -5

# Write a smoke test file (auto-generated, deleted after run)
cat > e2e-smoke.spec.ts << 'TEST'
import { test, expect } from '@playwright/test';

const BASE = 'http://localhost';

test.describe('UI smoke test', () => {
  test('login page loads without console errors', async ({ page }) => {
    const errors: string[] = [];
    page.on('console', msg => { if (msg.type() === 'error') errors.push(msg.text()); });
    page.on('pageerror', err => errors.push(err.message));
    await page.goto(BASE, { waitUntil: 'networkidle' });
    // Should see login/register — app is not authenticated
    await expect(page.locator('body')).toBeVisible();
    expect(errors).toEqual([]);
  });

  test('public image page returns 404 gracefully', async ({ page }) => {
    const errors: string[] = [];
    page.on('console', msg => { if (msg.type() === 'error') errors.push(msg.text()); });
    page.on('pageerror', err => errors.push(err.message));
    await page.goto(BASE + '/u/nonexistent', { waitUntil: 'networkidle' });
    // Should show a 404 page, not a blank crash
    await expect(page.locator('body')).toBeVisible();
    expect(errors).toEqual([]);
  });
});
TEST

# Run the smoke tests
npx playwright test e2e-smoke.spec.ts --browser=chromium --reporter=list 2>&1

# Clean up
rm -f e2e-smoke.spec.ts
```

**Failure handling**: If any E2E test fails, treat it as a CRITICAL finding. Diagnose console errors or page crashes, fix the root cause, and re-run. Do NOT proceed to Phase 5.

#### Step 5: Teardown

```bash
docker compose down
```

#### Step 6: Failure handling

**If any integration test fails:**
1. Diagnose the failure. Check logs: `docker compose logs api`, `docker compose logs worker`.
2. Treat as a CRITICAL review finding. Fix the root cause.
3. Re-verify: restart docker compose (`docker compose down && docker compose up --build -d`), re-run integration tests.
4. Do NOT proceed to Phase 5 until all integration tests pass.

**If Docker is not available** (e.g., CI without Docker, local dev without Docker installed):
- Skip Phase 4 but log a warning: "Integration tests skipped — Docker not available. Only unit tests verified."
- The unit tests (`cargo test --workspace` without DB) already serve as the primary correctness gate.

### Phase 5: Finish

Invoke `superpowers:finishing-a-development-branch` skill.

**Before presenting options, run final verification:**
- `cargo clippy --workspace -- -D warnings` ✅
- `cargo test --workspace` ✅ (all non-ignored tests pass)
- `npm run build` (if frontend changed) ✅
- **Version bump check**: if frontend changed, verify `web-ui/package.json` version matches `Cargo.toml` workspace version. If not bumped, update both now.
- **E2E browser test**: if frontend changed, verify Phase 4 E2E smoke test passed (Playwright + Chromium, no console errors).

**Present the 4 options:**
1. **Merge** — merge into base branch, clean up worktree.
2. **Create PR** — push branch, create GitHub PR, share link.
3. **Keep as-is** — leave branch and worktree for later.
4. **Discard** — delete branch and worktree (requires explicit "discard" confirmation).

### Phase 6: Post-Completion — Auto-Sync Documentation

After the feature is merged or the PR is created, automatically execute the mandatory post-phase step from `AGENTS.md` Rules:

1. Update `AGENTS.md` — sync version, migrations count, new API routes, architecture notes, config vars, crate boundaries.
2. Update `README.md` — sync version tagline, Features checklist, Project Structure tree, API endpoint tables, migrations count, config var table.
3. Update `.omo/summary/summary_and_next.md` — add a new "## {phase}: {title} ✅ (本次完成)" section documenting what was built, verification results, and updating the "## 待实施" table.
4. Commit the three files together as `docs: auto-sync AGENTS.md, README.md, summary after {phase} completion`.
5. Do NOT wait for the user to request this — it is mandatory.

---

## Size-Based Execution Strategy

| Tier | Git Isolation | TDD | Execution | Review | Integration Testing |
|------|--------------|-----|-----------|--------|---------------------|
| trivial | Feat branch on current repo | RED→GREEN→REFACTOR enforced | Inline (no subagent) | Self-review only | Skip (Docker optional) |
| standard | Feat branch on current repo | RED→GREEN→REFACTOR enforced | Subagent per task (sequential) | 2 reviewers (code-reviewer + language reviewer) | Full: docker compose + integration tests + E2E browser smoke test (if web-ui/) |
| large | Feat branch on current repo | RED→GREEN→REFACTOR enforced | Subagent per task (parallel independent, sequential dependent) | 3 reviewers (+ security-reviewer) | Full: docker compose + integration tests + smoke tests + E2E browser smoke test (if web-ui/) |

---

## Failure Recovery

### If a task implementer subagent fails:
- If failed once: investigate the error, provide more context to the subagent, retry.
- If failed twice: re-read the relevant files, adjust the task brief with more specific instructions, retry.
- If failed 3 times: **Consult Oracle** with full failure context. Do NOT continue blindly.
- If Oracle cannot resolve: report to user with what's been tried.

### If tests break after a task:
- Do NOT proceed to the next task. The current task must pass verification.
- If the breakage is in code the task didn't touch, report as a pre-existing issue.

### If the session is interrupted:
- Read `.superpowers/sdd/progress.md` on resume.
- Continue from the last completed task.
- The task briefs and review packages are self-contained — no lost context.

---

## Project-Specific Verification Gates

Apply these after **every task** and **before finish**:

```bash
# Backend
cargo clippy --workspace -- -D warnings   # Zero warnings required
cargo test --workspace                     # All non-ignored must pass

# Frontend (if web-ui/ changed)
cd web-ui && npm run build                 # tsc -b && vite build

# Version bump check (if web-ui/ changed)
# Verify web-ui/package.json version matches Cargo.toml workspace version.
# If not bumped, bump both: patch for fixes, minor for features.

# E2E browser smoke test (if web-ui/ changed)
# Runs in Phase 4 via docker compose. Verifies pages load and navigate
# without console errors using Playwright + Chromium.
```

**Integration tests** (11 tests in `pichost-api/tests/` require DB/Redis/S3): these run in Phase 4 via docker compose. Until Phase 4, they are expected to be skipped or fail — do NOT treat them as failures during Phase 2 unit testing.

---

## Project-Specific Conventions

Enforce these from `AGENTS.md`:
- **Commit messages**: English, semantic prefix (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`).
- **Rust**: functions ≤50 lines, lines ≤120 chars. Use `rust-refactor-fns` skill when functions grow.
- **DB queries**: run-time only (`query_as`, `query_scalar`). No `query!` macro. No compile-time DB.
- **Config**: use `PICHOST_` prefix env vars via `figment`. Add to `pichost-core/src/config.rs`.
- **New migrations**: create `migrations/XXXX_description.sql`. Number sequentially from highest existing.
- **Frontend**: Zustand (client state) + TanStack Query v5 (server state). ky for HTTP. react-router-dom v7.
- **Version bump**: patch for fixes, minor for features. Update `Cargo.toml` workspace version. If the feature touches `web-ui/`, also bump version in `web-ui/package.json` to match.