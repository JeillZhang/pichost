# User Storage Quota Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enforce per-user storage quotas — default 1 GB for new users, null for unlimited, admin-overridable — blocking uploads that would exceed quota, with usage bar in frontend.

**Architecture:** Add `storage_quota BIGINT NULL` to the users table (NULL = unlimited), add `storage_quota_default` to UploadConfig, inject quota into UserStats/UserInfo responses. In the upload pipeline, sum the user's existing storage and reject uploads that would exceed quota. Admin can view/edit per-user quota. Frontend shows a usage bar on Dashboard and an admin input for quota in the user edit dialog.

**Tech Stack:** Rust 1.96 (Axum 0.8, sqlx 0.8, serde, argon2), React 19, TypeScript 5.7, Tailwind CSS v4, TanStack Query v5

## Global Constraints

- Rust edition 2021, `cargo clippy --workspace -- -D warnings` must pass, `cargo test --workspace` must pass
- Frontend: `npm run build` (tsc + vite) must pass
- No new external Rust crates; no new npm dependencies
- Follow existing code patterns: inline `sqlx::query_as`, same error response format (`(StatusCode, Json(json!(...)))`)
- All new DB fields use `Option<i64>` in Rust for nullable BIGINT
- NULL quota = unlimited; negative values are invalid and should be rejected
- All commits in English, spec docs in Chinese

---

## File Structure

```
migrations/0006_add_storage_quota.sql              (CREATE) — ALTER TABLE add column
pichost-core/src/config.rs                          (MODIFY) — add storage_quota_default to UploadConfig
pichost-api/src/routes/users.rs                     (MODIFY) — add storage_quota to UserStats
pichost-api/src/routes/auth.rs                      (MODIFY) — add storage_quota to UserInfo
pichost-api/src/routes/admin.rs                     (MODIFY) — add quota to list/update, query column
pichost-api/src/services/upload.rs                  (MODIFY) — quota enforcement in process_upload
web-ui/src/api/client.ts                            (MODIFY) — add storage_quota to UserInfo, UserStats types
web-ui/src/pages/Dashboard.tsx                      (MODIFY) — usage bar below DropZone
web-ui/src/pages/Admin/index.tsx                    (MODIFY) — quota column + edit input
```

---

### Task 1: Database migration — add `storage_quota` column

**Files:**
- Create: `migrations/0006_add_storage_quota.sql`

- [ ] **Step 1: Create migration**

```sql
-- migrations/0006_add_storage_quota.sql
-- Add per-user storage quota (NULL = unlimited, bytes)
ALTER TABLE users ADD COLUMN IF NOT EXISTS storage_quota BIGINT;
COMMENT ON COLUMN users.storage_quota IS 'Per-user storage quota in bytes. NULL = unlimited.';
```

- [ ] **Step 2: Verify build embeds migration**

```bash
cargo build -p pichost-api
```

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add migrations/0006_add_storage_quota.sql
git commit -m "feat: add storage_quota column to users table"
```

---

### Task 2: Add `storage_quota_default` to UploadConfig

**Files:**
- Modify: `pichost-core/src/config.rs`

**Interfaces:**
- Produces: `UploadConfig.storage_quota_default: u64` (default 1_073_741_824 = 1 GB)
- Consumed by: Task 4 (upload enforcement), Task 3 (user registration default)

- [ ] **Step 1: Add field to UploadConfig struct**

In `pichost-core/src/config.rs`, add to UploadConfig (line 75):

```rust
pub struct UploadConfig {
    pub max_file_size_admin: u64,
    pub max_file_size_user: u64,
    pub allowed_mimes: Vec<String>,
    #[serde(default = "default_storage_quota")]
    pub storage_quota_default: u64,
}

fn default_storage_quota() -> u64 {
    1_073_741_824 // 1 GB
}
```

- [ ] **Step 2: Update Default impl**

In the `Default for AppConfig` block (line 136), add the quota field to UploadConfig:

```rust
upload: UploadConfig {
    max_file_size_admin: 52_428_800,
    max_file_size_user: 10_485_760,
    allowed_mimes: vec!["image/png".into(), "image/jpeg".into(), "image/gif".into(), "image/webp".into(), "image/svg+xml".into(), "image/avif".into(), "image/bmp".into()],
    storage_quota_default: 1_073_741_824,
},
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check -p pichost-core
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add pichost-core/src/config.rs
git commit -m "feat: add storage_quota_default to UploadConfig"
```

---

### Task 3: Add `storage_quota` to UserInfo and UserStats, update auth routes

**Files:**
- Modify: `pichost-api/src/routes/auth.rs` (UserInfo struct, registration INSERT)
- Modify: `pichost-api/src/routes/users.rs` (UserStats struct)

**Interfaces:**
- Consumes: UploadConfig.storage_quota_default from Task 2
- Produces: Updated UserInfo, UserStats with `storage_quota: Option<i64>`
- Consumed by: Task 6 (admin list/update), Task 7-8 (frontend)

- [ ] **Step 1: Update UserInfo in auth.rs**

Find the `UserInfo` struct in `pichost-api/src/routes/auth.rs` and add the field:

```rust
#[derive(Debug, Serialize, Deserialize)]
pub struct UserInfo {
    pub id: Uuid,
    pub username: String,
    pub email: Option<String>,
    pub is_admin: bool,
    pub storage_quota: Option<i64>,
}
```

Also update the `created_at` field if present — check the actual struct first.

- [ ] **Step 2: Update login handler SELECT query**

Find the login handler's `SELECT` query that returns user info. Add `storage_quota` to the column list:

```sql
SELECT id, username, email, is_admin, storage_quota FROM users WHERE username = $1
```

Update the row destructuring to include `storage_quota`.

- [ ] **Step 3: Update registration INSERT**

In the register handler, add `storage_quota` to the INSERT, using the config default:

```rust
let storage_quota = if state.config.upload.storage_quota_default > 0 {
    Some(state.config.upload.storage_quota_default as i64)
} else {
    None
};

sqlx::query(
    "INSERT INTO users (username, password_hash, is_admin, storage_quota) VALUES ($1, $2, $3, $4)"
)
.bind(&username)
.bind(&password_hash)
.bind(is_admin)
.bind(storage_quota)
```

- [ ] **Step 4: Update UserStats in users.rs**

```rust
#[derive(Debug, Serialize)]
pub struct UserStats {
    pub total_images: i64,
    pub total_size: i64,
    pub backend: String,
    pub storage_quota: Option<i64>,
}
```

In the handler, fetch the user's quota:

```rust
let quota: Option<i64> = sqlx::query_scalar(
    "SELECT storage_quota FROM users WHERE id = $1"
)
.bind(user.id)
.fetch_optional(&state.pool)
.await
.map_err(|e| {
    tracing::warn!("Quota query failed: {e}");
    (StatusCode::INTERNAL_SERVER_ERROR, Json(serde_json::json!({"error": "internal server error"})))
})?
.flatten();

Ok(Json(UserStats {
    total_images: row.0,
    total_size: row.1.unwrap_or(0),
    backend: state.router.default_name().to_string(),
    storage_quota: quota,
}))
```

- [ ] **Step 5: Verify compilation and tests**

```bash
cargo check -p pichost-api
cargo clippy --workspace -- -D warnings
```

Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add pichost-api/src/routes/auth.rs pichost-api/src/routes/users.rs
git commit -m "feat: add storage_quota to UserInfo, UserStats, and registration"
```

---

### Task 4: Quota enforcement in upload pipeline

**Files:**
- Modify: `pichost-api/src/services/upload.rs`

**Interfaces:**
- Consumes: UploadConfig.storage_quota_default from Task 2, storage_quota column from Task 1
- Produces: Quota check returns 413 Payload Too Large if exceeded

- [ ] **Step 1: Add quota check after file size validation**

In `process_upload`, after the existing file size check (max_file_size_admin/User), add:

```rust
// Check storage quota (if set)
if let Some(quota) = user.storage_quota {
    let current_usage: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(file_size), 0) FROM images WHERE user_id = $1"
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Quota usage query failed: {e}");
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"})))
    })?;
    
    if current_usage + file_size > quota {
        return Err((
            StatusCode::PAYLOAD_TOO_LARGE,
            Json(json!({
                "error": "storage quota exceeded",
                "quota_bytes": quota,
                "used_bytes": current_usage,
                "file_bytes": file_size,
            })),
        ));
    }
}
```

Note: `user.storage_quota` needs to be available. Currently `AuthUser` may not carry `storage_quota`. Read the `AuthUser` struct in `middleware/auth.rs` — if it only has `id` and `is_admin`, add `storage_quota: Option<i64>` to it and populate from the JWT or from a DB lookup.

The simplest approach: add a DB query for the user's quota inside `process_upload` (or add `storage_quota` to `AuthUser` via JWT claims or a lookup in the auth middleware).

Preferred: Add `storage_quota` to AuthUser. In the auth middleware, after verifying the JWT, also query the user's quota:

```rust
// In middleware/auth.rs, in the require_auth function
let row = sqlx::query_as::<_, (Uuid, bool, Option<i64>)>(
    "SELECT id, is_admin, storage_quota FROM users WHERE id = $1"
)
.bind(user_id)
.fetch_one(&pool).await?;

// Return AuthUser { id: row.0, is_admin: row.1, storage_quota: row.2 }
```

- [ ] **Step 2: Verify compilation**

```bash
cargo check -p pichost-api
```

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add pichost-api/src/services/upload.rs pichost-api/src/middleware/auth.rs
git commit -m "feat: enforce storage quota in upload pipeline"
```

---

### Task 5: Admin — include quota in list_users and update_user

**Files:**
- Modify: `pichost-api/src/routes/admin.rs`

**Interfaces:**
- Consumes: Updated UserInfo from Task 3
- Produces: Admin can view and edit per-user storage_quota

- [ ] **Step 1: Update list_users query**

Add `storage_quota` to the SELECT in `list_users`:

```rust
let rows = sqlx::query_as::<_, (Uuid, String, Option<String>, bool, String, Option<i64>, chrono::DateTime<chrono::Utc>)>(
    r#"SELECT id, username, email, is_admin, storage_backend, storage_quota, created_at
       FROM users ORDER BY created_at DESC OFFSET $1 LIMIT $2"#,
)
```

Update the row mapping to include `storage_quota` in UserInfo.

- [ ] **Step 2: Add storage_quota to UpdateUserBody**

```rust
#[derive(Debug, Deserialize)]
pub struct UpdateUserBody {
    pub username: Option<String>,
    pub email: Option<String>,
    pub password: Option<String>,
    pub is_admin: Option<bool>,
    pub storage_backend: Option<String>,
    pub storage_quota: Option<i64>,
}
```

In the `update_user` handler:
- Fetch existing `storage_quota` alongside other fields in the SELECT
- Use `body.storage_quota.unwrap_or(existing_quota)` 
- Validate: if provided, must be `>= 0` or `null` (negative quota is invalid)
- Include in UPDATE SQL for both branches (with/without password)

- [ ] **Step 3: Verify compilation**

```bash
cargo check -p pichost-api
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add pichost-api/src/routes/admin.rs
git commit -m "feat: add storage_quota to admin list_users and update_user"
```

---

### Task 6: Frontend — update API types and add quota to UserInfo

**Files:**
- Modify: `web-ui/src/api/client.ts`

- [ ] **Step 1: Add storage_quota to TypeScript types**

```typescript
export interface UserInfo {
  id: string
  username: string
  email?: string | null
  is_admin: boolean
  storage_quota: number | null
}

export interface UserStats {
  total_images: number
  total_size: number
  backend: string
  storage_quota: number | null
}
```

- [ ] **Step 2: Verify TypeScript compilation**

```bash
cd web-ui && npx tsc --noEmit
```

Expected: Will show type errors in admin pages — that's expected, fixed in Task 7.

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/api/client.ts
git commit -m "feat: add storage_quota to UserInfo and UserStats frontend types"
```

---

### Task 7: Frontend — usage bar on Dashboard

**Files:**
- Modify: `web-ui/src/pages/Dashboard.tsx`

- [ ] **Step 1: Add user stats query and usage bar**

Add a `useQuery` for user stats after the existing `images` query, and render a usage bar below DropZone:

```tsx
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { getUserStats } from '../api/client'

// Add this after the existing images query
const { data: stats } = useQuery({
  queryKey: ['user-stats'],
  queryFn: () => getUserStats(),
})

// Render usage bar between queue and recent images
{stats && stats.storage_quota != null && (
  <div className="mt-4 rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-3 backdrop-blur-sm">
    <div className="mb-1 flex items-center justify-between text-xs">
      <span className="text-[var(--color-text-muted)]">Storage</span>
      <span className="text-[var(--color-text-secondary)]">
        {formatBytes(stats.total_size)} / {formatBytes(stats.storage_quota)}
      </span>
    </div>
    <div className="h-2 overflow-hidden rounded-full bg-[var(--color-border)]">
      <div
        className="h-full rounded-full transition-all duration-500"
        style={{
          width: `${Math.min(100, (stats.total_size / stats.storage_quota) * 100)}%`,
          backgroundColor: stats.total_size / stats.storage_quota > 0.9
            ? 'var(--color-error)'
            : stats.total_size / stats.storage_quota > 0.7
              ? 'var(--color-warning, #f59e0b)'
              : 'var(--color-accent)',
        }}
      />
    </div>
  </div>
)}
```

Add a `getUserStats` function to `client.ts`:

```typescript
export async function getUserStats(): Promise<UserStats> {
  return api.get('users/me/stats').json<UserStats>()
}
```

Add `formatBytes` helper:

```typescript
function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  return `${(bytes / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`
}
```

NOTE: `getUserStats` already exists? Check `client.ts` — if not, add it. If it exists under a different name, use that.

- [ ] **Step 2: Verify TypeScript and build**

```bash
cd web-ui && npx tsc --noEmit
cd web-ui && npm run build
```

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/pages/Dashboard.tsx web-ui/src/api/client.ts
git commit -m "feat: add storage usage bar to Dashboard"
```

---

### Task 8: Frontend — admin quota input in user edit dialog

**Files:**
- Modify: `web-ui/src/pages/Admin/index.tsx` (or wherever the admin user table/edit dialog is)

Check the actual admin page structure — read `web-ui/src/pages/Admin/` first. Likely pattern:

- [ ] **Step 1: Add quota column to user table**

Add a "Quota" column header and cell in the user table rows, showing "Unlimited" for null or formatted bytes:

```tsx
<th>Quota</th>
...
<td>{user.storage_quota != null ? formatBytes(user.storage_quota) : 'Unlimited'}</td>
```

- [ ] **Step 2: Add quota input to the edit user dialog**

In the edit user form/modal, add a storage quota field:

```tsx
<div>
  <label className="block text-sm text-[var(--color-text-secondary)] mb-1">
    Storage Quota (bytes, 0 = unlimited)
  </label>
  <input
    type="number"
    min="0"
    value={editQuota ?? ''}
    onChange={(e) => setEditQuota(e.target.value ? Number(e.target.value) : null)}
    placeholder="Unlimited"
    className="w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-3 py-2 text-sm text-[var(--color-text-primary)] backdrop-blur-sm"
  />
  <p className="mt-1 text-xs text-[var(--color-text-muted)]">
    {editQuota != null && editQuota > 0 ? formatBytes(editQuota) : 'No limit'}
  </p>
</div>
```

Include `storage_quota` in the PATCH request body when saving.

- [ ] **Step 3: Verify TypeScript and build**

```bash
cd web-ui && npx tsc --noEmit
cd web-ui && npm run build
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add web-ui/src/pages/Admin/
git commit -m "feat: add storage quota column and edit input to admin panel"
```

---

### Task 9: Integration smoke test + spec/summary update + version bump

**Files:**
- Modify: `docs/superpowers/specs/2026-07-11-pichost-design.md`
- Modify: `.omo/summary/summary_and_next.md`
- Modify: `Cargo.toml`

- [ ] **Step 1: Full verification**

```bash
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace
cd web-ui && npm run build
```

Expected: All pass.

- [ ] **Step 2: Update spec TODO**

```markdown
- [x] 用户存储配额 (storage_quota BIGINT NULL, default 1GB, upload enforcement, admin management, Dashboard usage bar)
```

- [ ] **Step 3: Update summary**

Add completion entry and bump recommended next to "OAuth 登录 或 批量管理".

- [ ] **Step 4: Bump version**

```toml
version = "0.10.0"
```

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/specs/2026-07-11-pichost-design.md .omo/summary/summary_and_next.md Cargo.toml Cargo.lock docs/superpowers/plans/
git commit -m "chore: update spec and summary for storage quota, bump version to 0.10.0"
```

---

## Self-Review Checklist

### 1. Spec Coverage
- ✅ DB migration: Task 1
- ✅ Config default: Task 2
- ✅ UserInfo/UserStats updated: Task 3
- ✅ Upload enforcement: Task 4
- ✅ Admin view/edit: Task 5
- ✅ Frontend API types: Task 6
- ✅ Dashboard usage bar: Task 7
- ✅ Admin quota input: Task 8
- ✅ Smoke test: Task 9

### 2. Placeholder Scan
- ✅ No "TBD", "TODO"
- ✅ All code inline
- ✅ Edge cases: null quota (unlimited), negative values rejected, zero byte files

### 3. Type Consistency
- ✅ `storage_quota: Option<i64>` consistent across DB, Rust, TypeScript
- ✅ `storage_quota_default: u64` in config, cast to `i64` when inserting
- ✅ quota = null → unlimited; quota = 0 → effectively blocks uploads (valid edge case)
