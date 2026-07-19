---
description: Orchestrate superpowers brainstorming to produce a design spec from a feature description
argument-hint: "[feature description]"
---

# feature-design — Design Phase Orchestrator

You are orchestrating the **design phase** of a feature. Your job is to take a feature description, run the `superpowers:brainstorming` pipeline, and produce an approved design spec ready for `/feature-plan`.

The feature description is: `$ARGUMENTS`

---

## Pipeline Phases

### Phase 0: Intake & Classify

1. Read `.omo/summary/summary_and_next.md` to understand current project state and pending features.
2. Read `docs/superpowers/specs/` for any relevant existing design docs.
3. Restate the feature request in your own words. Classify its size:
   - **trivial**: single endpoint, single component, known pattern
   - **standard**: multi-file, new module, new DB migration
   - **large**: cross-cutting, new storage backend, new auth flow, multi-crate

### Phase 1: Research

For **standard** and **large** features:
1. Fire 2-3 parallel `explore` agents to find existing patterns in the codebase:
   - One for backend patterns (routes, services, models, DB queries)
   - One for frontend patterns (components, hooks, API client)
   - One for relevant config/middleware/storage patterns
2. If unfamiliar libraries/APIs are involved, fire a `librarian` agent for external docs.

### Phase 2: Brainstorming → Design Spec

Invoke `superpowers:brainstorming` skill with the feature description.

**Key rules from brainstorming:**
- Ask clarifying questions **one at a time** (never batch questions).
- Propose 2-3 design approaches with tradeoffs.
- Present design section-by-section for incremental approval.
- Output: `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`
- The spec must use UML or Mermaid diagrams (per AGENTS.md Rules).
- The spec must be written in Chinese (per AGENTS.md Rules: `docs/superpowers/specs/` docs in Chinese).

**Hard gate:** Do NOT write any code. This phase produces a design document only.

### Phase 3: Gate

After the design spec is approved by the user:

1. Confirm the spec file is committed: `docs/superpowers/specs/YYYY-MM-DD-<topic>-design.md`.
2. Present a summary: key architecture decisions, scope boundary, areas that need deeper design.
3. **GATE**: Ask the user to confirm the spec is complete.
4. Tell the user: "Design spec is ready. Run `/feature-plan docs/superpowers/specs/<spec-file>.md` to produce an implementation plan."

---

## Size Classification & Phase Mask

| Tier | Criteria | Phases | Parallel Research |
|------|----------|--------|-------------------|
| trivial | single endpoint, known pattern | 0 → 2 → 3 | Skip Phase 1 |
| standard | multi-file, new module, new DB migration | 0 → 1 → 2 → 3 | 2-3 explore agents |
| large | cross-cutting, new storage backend, new auth flow | 0 → 1 → 2 → 3 | 3+ explore + librarian |

---

## Project-Specific Context

Always apply these from `AGENTS.md`:
- **Rust**: functions ≤50 lines, lines ≤120 chars. Use `rust-refactor-fns` skill when needed.
- **Crate boundaries**: pichost-core (models/traits/storage), pichost-api (routes/services), pichost-worker (bg processing).
- **Config**: use `PICHOST_` prefix env vars via `figment`. Add to `pichost-core/src/config.rs`.
- **DB**: migrations auto-apply via `sqlx::migrate!()`. Run-time queries only (no `query!` macro).
- **Frontend**: React 19, Vite 8, Tailwind CSS 4, TypeScript 7. Zustand + TanStack Query v5.
- **Verification gates**: `cargo clippy --workspace -- -D warnings` + `cargo test --workspace` + `npm run build`.
- **Commit style**: semantic (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`), messages in English.
- **Version bump**: patch for fixes, minor for features.