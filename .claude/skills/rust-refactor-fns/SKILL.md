---
name: rust-refactor-fns
description: >
  Refactor oversized Rust functions and over-width lines across a Cargo workspace.
  Use this skill whenever the user asks to reduce function sizes, enforce function
  length limits, fix line-width violations, break up large functions, or generally
  refactor Rust code for readability. Triggers on phrases like
  "函数太大", "refactor large functions", "split oversized functions",
  "enforce 50-line limit", "lines too long", "超过120字符",
  "break down this function", or any request to extract helpers from a Rust
  codebase. Even if the user doesn't say "refactor" explicitly — when they say
  "this function is too long" or "can you split this up", use this skill.
---

# Rust Function Refactoring Skill

Systematic workflow for detecting and eliminating oversized Rust functions and
over-width lines across an entire Cargo workspace. Proven on a 15,000-line
codebase — 100+ functions extracted, zero clippy warnings sustained throughout.

## Workflow

### Phase 1: Audit

Run both detection scripts from the workspace root:

```bash
python3 .claude/skills/rust-refactor-fns/scripts/find_large_fns.py --threshold 50
python3 .claude/skills/rust-refactor-fns/scripts/find_long_lines.py --threshold 120
```

The first script parses Rust source files with a state-machine brace counter
(correctly handles multi-line signatures, nested braces, trait methods ending
with `;`). The second is a simple length check.

Record the results: file, function name, line count for every function >50
lines. Group by file — this determines your delegation strategy.

### Phase 2: Priority

Sort all oversized functions by line count descending. Over ~150 lines →
"critical", 75–150 → "high", 50–75 → "medium". Critical functions should get
their own subagent; medium ones can be batched per file.

### Phase 3: Dispatch (PARALLEL)

For each **independent file**, spawn a `deep` agent with a prompt that includes:

1. **TASK**: Atomic goal — one file per agent, list specific functions
2. **EXPECTED OUTCOME**: Target line counts per function (e.g. "register 124→50")
3. **MUST DO**: Exact helper signatures to extract, verification commands
4. **MUST NOT DO**: Files to leave untouched, behaviors to preserve
5. **CONTEXT**: File path, existing patterns, relevant types

Pattern for the prompt:

```
TASK: Refactor `path/to/file.rs` — break down N oversized functions.

EXPECTED OUTCOME:
- `function_name` (~124L) → <50 lines: extract
  - `helper_one(params) -> ReturnType` — what it does
  - `helper_two(params) -> ReturnType` — what it does

REQUIRED TOOLS: read, edit, lsp_diagnostics, bash (cargo check)
MUST DO:
1. Read the full file first
2. Extract helpers with these exact responsibilities: ...
3. Each helper must be private, <40 lines
4. Preserve exact business logic, error messages, SQL queries
5. Run `cargo check -p CRATE 2>&1` then `cargo clippy -p CRATE -- -D warnings 2>&1`
MUST NOT DO:
- Change public function signatures
- Modify files other than this one
- Change error messages, status codes, or business logic
```

Fire all agents with `run_in_background=true`. They work on independent files
so there are no merge conflicts.

### Phase 4: Verify

After all agents finish, run the full suite:

```bash
cargo check --workspace
cargo clippy --workspace -- -D warnings
cargo test --workspace
cd web-ui && npm run build  # if frontend exists
```

Then re-run the detection scripts to confirm zero results.

### Phase 5: Iterate

Invariably a few functions slip through (agents add new helpers that exceed
the threshold, or compress functions imperfectly). Run the detection again,
dispatch a second wave. Each iteration tackles fewer files. Usually 2–3
waves total for a 50+ function codebase.

## Common Refactoring Patterns

### Pattern 1: Extract validation helpers

```rust
// BEFORE: 30 lines of validation inline
pub async fn handler(...) {
    if condition { return Err(...); }
    if other_condition { return Err(...); }
    // ... more checks ...
}

// AFTER: single call
pub async fn handler(...) {
    validate_request(&payload)?;
    // ...
}
async fn validate_request(p: &Payload) -> Result<(), Error> { ... }
```

### Pattern 2: Extract query helpers

```rust
// BEFORE: 15 lines of SQL + error mapping inline
let rows = sqlx::query_as::<_, RowType>("SELECT ...")
    .bind(...).fetch_all(&pool).await
    .map_err(|e| { tracing::warn!(...); (500, Json(...)) })?;

// AFTER: helper call
let rows = fetch_rows(&pool, user_id).await?;

async fn fetch_rows(pool: &PgPool, uid: Uuid) -> Result<Vec<Row>, Error> { ... }
```

### Pattern 3: Extract response-builders

```rust
// BEFORE: 20 lines constructing a JSON response
let result = UploadResult {
    id, public_key, original_name: name.clone(),
    url: url.clone(),
    markdown: format!("![{}]({})", name, url),
    // ... 10 more fields ...
};

// AFTER: constructor method
impl UploadResult {
    fn from_row(row: ImageRow) -> Self { ... }
}
```

### Pattern 4: Sub-route extractors (for main.rs / router setup)

```rust
// BEFORE: 100-line build_router with 6 inline route groups
fn build_router(state: Arc<AppState>) -> Router {
    let auth_routes = Router::new().route(...)... // 12 lines
    let upload_routes = Router::new().route(...)... // 8 lines
    let image_routes = Router::new().route(...)... // 12 lines
    // ... 3 more groups ...
    Router::new().nest("/api/v1/auth", auth_routes)...
}

// AFTER: each group is its own function
fn auth_routes(state: Arc<AppState>) -> Router { ... }
fn upload_routes(state: Arc<AppState>, protected: &Layer) -> Router { ... }
fn build_router(state: Arc<AppState>) -> Router {
    let protected = ...;
    Router::new()
        .nest("/api/v1/auth", auth_routes(state.clone()))
        .nest("/api/v1/images", upload_routes(state.clone(), &protected))
        // ... clean, <20 lines
}
```

### Pattern 5: Error-type aliases

When multiple helpers share the same error type, define a module-level alias:

```rust
type ApiError = (StatusCode, Json<serde_json::Value>);
// Then use `Result<T, ApiError>` throughout all helpers
```

## Helper Size Rule

Extracted helpers should be 20–40 lines. If a helper exceeds 40 lines, split it
further. If it's under 10 lines and called only once, consider inlining it.
The sweet spot: a function that does one thing, fits on one screen, and needs
no scrolling to understand.

## Line Width Fixes

For lines >120 characters, prefer these approaches in order:

1. **Unwrap struct literals**: One field per line instead of inline
2. **Break long SQL strings**: Use `\` line continuation in Rust
3. **Break long function calls**: One argument per line
4. **Format macro invocations**: One field per line (e.g., `tracing::info!`)

Example:

```rust
// BEFORE (168 chars)
server: ServerConfig { host: "0.0.0.0".into(), port: 3000, public_url: "http://localhost:3000".into() },

// AFTER (max 40 chars per line)
server: ServerConfig {
    host: "0.0.0.0".into(),
    port: 3000,
    public_url: "http://localhost:3000".into(),
},
```

## Bundled Scripts

- `scripts/find_large_fns.py` — detects functions > N lines (default 50)
- `scripts/find_long_lines.py` — detects lines > N chars (default 120)

Both accept `--threshold N` and `--dir PATH` arguments.
