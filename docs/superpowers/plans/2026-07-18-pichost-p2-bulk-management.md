# Bulk Image Management Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable selecting multiple images in Gallery and batch-deleting them via a single API call, with a confirmation dialog and selection toolbar.

**Architecture:** Add `POST /api/v1/images/batch-delete { ids: UUID[] }` backend endpoint that validates ownership, batch-deletes storage files (best-effort), and removes DB records in a transaction. Frontend adds selection mode to Gallery: click to toggle selection, toolbar with "Select All"/"Delete Selected" buttons, confirmation dialog with count.

**Tech Stack:** Rust 1.96 (Axum 0.8, sqlx 0.8), React 19, TypeScript 5.7, Tailwind CSS v4, TanStack Query v5

## Global Constraints

- Rust edition 2021, `cargo clippy --workspace -- -D warnings` must pass, `cargo test --workspace` must pass
- Frontend: `npm run build` (tsc + vite) must pass
- No new external Rust crates; no new npm dependencies
- Follow existing code patterns: inline `sqlx::query_as`, same error response format
- Reuse existing `delete_image` logic where possible (storage key collection + deletion)
- All commits in English, spec docs in Chinese
- React 19 + TypeScript 5.7 strict mode

---

## File Structure

```
pichost-api/src/routes/images.rs          (MODIFY) — add batch_delete handler
pichost-api/src/main.rs                   (MODIFY) — register /batch-delete route
web-ui/src/api/client.ts                  (MODIFY) — add batchDeleteImages function
web-ui/src/pages/Gallery.tsx              (MODIFY) — selection mode + toolbar
```

---

### Task 1: Backend — batch_delete handler

**Files:**
- Modify: `pichost-api/src/routes/images.rs` (append after `delete_image`)
- Modify: `pichost-api/src/main.rs` (register route)

**Interfaces:**
- Consumes: Existing `delete_image` logic pattern, `AuthUser` from middleware
- Produces: `POST /api/v1/images/batch-delete` accepting `{ ids: UUID[] }`, returning `{ deleted: usize, failed: usize }`
- Consumed by: Task 3 (frontend API client)

- [ ] **Step 1: Add request type and handler**

Append to `pichost-api/src/routes/images.rs`:

```rust
use serde::Deserialize;

#[derive(Debug, Deserialize)]
pub struct BatchDeleteRequest {
    pub ids: Vec<Uuid>,
}

/// POST /api/v1/images/batch-delete — delete multiple images (protected)
pub async fn batch_delete(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<BatchDeleteRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    if body.ids.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "no image ids provided"})),
        ));
    }
    if body.ids.len() > 100 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "batch limit is 100 images"})),
        ));
    }

    // Validate ownership: collect storage keys for user's images only
    let rows: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        r#"SELECT storage_key, storage_backend, thumbnail_key, webp_key
           FROM images WHERE id = ANY($1) AND (user_id = $2 OR $3)"#,
    )
    .bind(&body.ids)
    .bind(user.id)
    .bind(user.is_admin)
    .fetch_all(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Batch delete query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal server error"})),
        )
    })?;

    // Delete storage files (best-effort)
    let mut storage_deleted = 0usize;
    for (storage_key, storage_backend, thumb_key, webp_key) in &rows {
        let backend = state.router.for_backend(storage_backend);
        if backend.delete(storage_key).await.is_ok() {
            storage_deleted += 1;
        }
        if let Some(ref tk) = thumb_key {
            let _ = backend.delete(tk).await;
        }
        if let Some(ref wk) = webp_key {
            let _ = backend.delete(wk).await;
        }
    }

    // Batch delete from DB in a transaction
    let deleted = sqlx::query("DELETE FROM images WHERE id = ANY($1)")
        .bind(&body.ids)
        .execute(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Batch delete DB failed: {e}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"error": "failed to delete images"})),
            )
        })?
        .rows_affected() as usize;

    let failed = body.ids.len().saturating_sub(deleted);

    tracing::info!(
        user_id = %user.id,
        requested = body.ids.len(),
        deleted,
        failed,
        "batch delete completed"
    );

    Ok(Json(json!({
        "message": "batch delete completed",
        "deleted": deleted,
        "failed": failed,
    })))
}
```

- [ ] **Step 2: Register route in main.rs**

Find the `image_routes` router (around line 94). Add the batch-delete route:

```rust
let image_routes = Router::new()
    .route("/", get(routes::images::list_images))
    .route(
        "/{id}",
        get(routes::images::get_image).delete(routes::images::delete_image),
    )
    .route("/batch-delete", post(routes::images::batch_delete))
    // ... existing route_layer lines
```

- [ ] **Step 3: Verify compilation and tests**

```bash
cargo check -p pichost-api
cargo clippy --workspace -- -D warnings
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add pichost-api/src/routes/images.rs pichost-api/src/main.rs
git commit -m "feat: add POST /images/batch-delete endpoint for bulk image deletion"
```

---

### Task 2: Frontend — API client batch delete function

**Files:**
- Modify: `web-ui/src/api/client.ts`

**Interfaces:**
- Consumes: Backend endpoint from Task 1
- Produces: `batchDeleteImages(ids: string[]): Promise<BatchDeleteResult>`
- Consumed by: Task 3 (Gallery)

- [ ] **Step 1: Add batchDeleteImages function**

```typescript
export interface BatchDeleteResult {
  message: string
  deleted: number
  failed: number
}

export async function batchDeleteImages(ids: string[]): Promise<BatchDeleteResult> {
  return api.post('images/batch-delete', { json: { ids } }).json<BatchDeleteResult>()
}
```

- [ ] **Step 2: Verify TypeScript**

```bash
cd web-ui && npx tsc --noEmit
```

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/api/client.ts
git commit -m "feat: add batchDeleteImages API function"
```

---

### Task 3: Frontend — Gallery selection mode + toolbar

**Files:**
- Modify: `web-ui/src/pages/Gallery.tsx`

**Interfaces:**
- Consumes: `batchDeleteImages` from Task 2, existing Gallery state
- Produces: Gallery with checkbox selection, toolbar, batch delete

- [ ] **Step 1: Rewrite Gallery.tsx with selection mode**

Replace `web-ui/src/pages/Gallery.tsx`:

```tsx
import { useRef, useCallback, useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useInfiniteQuery, keepPreviousData, useQueryClient } from '@tanstack/react-query'
import { listImages, batchDeleteImages } from '../api/client'
import type { ImageInfo, PaginatedListParams } from '../api/client'
import { CheckSquare, Square, Trash2, X } from 'lucide-react'
import SearchBar from '../components/SearchBar'
import SortDropdown from '../components/SortDropdown'

export default function Gallery() {
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [search, setSearch] = useState('')
  const [sort, setSort] = useState<NonNullable<PaginatedListParams['sort']>>('created_at')
  const [order, setOrder] = useState<NonNullable<PaginatedListParams['order']>>('desc')

  // Selection state
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [selectMode, setSelectMode] = useState(false)
  const [isDeleting, setIsDeleting] = useState(false)
  const [showConfirm, setShowConfirm] = useState(false)

  const {
    data,
    isLoading,
    isError,
    fetchNextPage,
    hasNextPage,
    isFetchingNextPage,
  } = useInfiniteQuery({
    queryKey: ['images', { search, sort, order }],
    queryFn: ({ pageParam }) =>
      listImages({ page: pageParam, per_page: 20, sort, order, search }),
    initialPageParam: 1,
    getNextPageParam: (lastPage) => {
      if (lastPage.page < lastPage.total_pages) return lastPage.page + 1
      return undefined
    },
    placeholderData: keepPreviousData,
  })

  const observerRef = useRef<IntersectionObserver>(undefined)
  const lastItemRef = useCallback(
    (node: HTMLButtonElement | null) => {
      if (isFetchingNextPage) return
      if (observerRef.current) observerRef.current.disconnect()
      observerRef.current = new IntersectionObserver(
        (entries) => {
          if (entries[0].isIntersecting && hasNextPage) fetchNextPage()
        },
        { rootMargin: '200px' },
      )
      if (node) observerRef.current.observe(node)
    },
    [isFetchingNextPage, hasNextPage, fetchNextPage],
  )

  useEffect(() => {
    return () => { observerRef.current?.disconnect() }
  }, [])

  const allImages: ImageInfo[] = data?.pages.flatMap((p) => p.items) ?? []
  const total = data?.pages[0]?.total ?? 0

  // Toggle single image selection
  function toggleSelect(id: string) {
    setSelected((prev) => {
      const next = new Set(prev)
      if (next.has(id)) {
        next.delete(id)
        if (next.size === 0) setSelectMode(false)
      } else {
        next.add(id)
        setSelectMode(true)
      }
      return next
    })
  }

  // Select/deselect all currently loaded images
  function toggleSelectAll() {
    if (selected.size === allImages.length) {
      setSelected(new Set())
      setSelectMode(false)
    } else {
      setSelected(new Set(allImages.map((img) => img.id)))
    }
  }

  function clearSelection() {
    setSelected(new Set())
    setSelectMode(false)
  }

  // Confirm dialog handlers
  function openConfirm() {
    if (selected.size === 0) return
    setShowConfirm(true)
  }

  async function confirmDelete() {
    setShowConfirm(false)
    setIsDeleting(true)
    try {
      const ids = Array.from(selected)
      const result = await batchDeleteImages(ids)
      if (result.deleted > 0) {
        queryClient.invalidateQueries({ queryKey: ['images'] })
      }
      clearSelection()
    } catch {
      // error already handled by ky hooks
    } finally {
      setIsDeleting(false)
    }
  }

  return (
    <div className="mx-auto max-w-5xl p-4">
      {/* Header row */}
      <div className="mb-4 flex flex-col gap-3 sm:flex-row sm:items-center sm:justify-between">
        <h1 className="text-lg font-bold text-[var(--color-text-primary)]">
          Gallery
          {total > 0 && (
            <span className="ml-2 text-sm font-normal text-[var(--color-text-muted)]">
              ({total} images)
            </span>
          )}
        </h1>
        <div className="flex items-center gap-3">
          <div className="w-48 sm:w-64">
            <SearchBar value={search} onChange={setSearch} />
          </div>
          <SortDropdown
            sort={sort}
            order={order}
            onSortChange={(s) => setSort(s as NonNullable<PaginatedListParams['sort']>)}
            onOrderChange={(o) => setOrder(o as NonNullable<PaginatedListParams['order']>)}
          />
        </div>
      </div>

      {/* Selection toolbar */}
      {selectMode && (
        <div className="mb-3 flex items-center justify-between rounded-lg border border-[var(--color-accent)] bg-[var(--color-accent-subtle)] px-3 py-2">
          <span className="text-sm text-[var(--color-text-primary)]">
            {selected.size} selected
          </span>
          <div className="flex items-center gap-2">
            <button
              onClick={toggleSelectAll}
              className="rounded px-2 py-1 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-surface)]"
            >
              {selected.size === allImages.length ? 'Deselect All' : 'Select All'}
            </button>
            <button
              onClick={openConfirm}
              disabled={isDeleting}
              className="flex items-center gap-1 rounded px-2 py-1 text-xs text-red-400 hover:bg-red-950 hover:text-red-300 disabled:opacity-50"
            >
              <Trash2 className="h-3 w-3" />
              Delete
            </button>
            <button
              onClick={clearSelection}
              className="rounded p-1 text-[var(--color-text-muted)] hover:text-[var(--color-text-primary)]"
            >
              <X className="h-4 w-4" />
            </button>
          </div>
        </div>
      )}

      {/* States */}
      {isLoading && (
        <div className="flex min-h-[200px] items-center justify-center text-[var(--color-text-muted)]">
          Loading…
        </div>
      )}
      {isError && (
        <div className="flex min-h-[200px] items-center justify-center text-red-500">
          Failed to load images.
        </div>
      )}
      {!isLoading && !isError && allImages.length === 0 && (
        <div className="flex min-h-[200px] flex-col items-center justify-center gap-2 text-[var(--color-text-muted)]">
          <p>No images found.</p>
          {search && <p className="text-sm">Try a different search term.</p>}
        </div>
      )}

      {/* Image grid */}
      {allImages.length > 0 && (
        <>
          <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4">
            {allImages.map((img, index) => {
              const isLast = index === allImages.length - 1
              const isSelected = selected.has(img.id)
              return (
                <div key={img.id} className="relative group">
                  {/* Selection checkbox (top-left corner) */}
                  {selectMode && (
                    <button
                      onClick={(e) => { e.stopPropagation(); toggleSelect(img.id) }}
                      className="absolute left-2 top-2 z-10 rounded bg-black/60 p-0.5 hover:bg-black/80"
                    >
                      {isSelected ? (
                        <CheckSquare className="h-4 w-4 text-[var(--color-accent)]" />
                      ) : (
                        <Square className="h-4 w-4 text-white/60" />
                      )}
                    </button>
                  )}

                  {/* Image button */}
                  <button
                    ref={isLast ? lastItemRef : undefined}
                    onClick={() => {
                      if (selectMode) {
                        toggleSelect(img.id)
                      } else {
                        navigate(`/images/${img.id}`)
                      }
                    }}
                    className={`aspect-square w-full overflow-hidden rounded-lg border bg-[var(--color-surface-glass)] backdrop-blur-sm transition-all ${
                      isSelected
                        ? 'border-[var(--color-accent)] ring-2 ring-[var(--color-accent)]'
                        : 'border-[var(--color-border)] hover:border-[var(--color-border-hover)]'
                    }`}
                  >
                    <img
                      src={img.thumbnail_url ?? img.url}
                      alt={img.original_name}
                      className="h-full w-full object-cover"
                      loading="lazy"
                    />
                    <div className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/80 to-transparent p-2">
                      <p className="truncate text-xs text-white">
                        {img.original_name}
                      </p>
                    </div>
                  </button>
                </div>
              )
            })}
          </div>

          {isFetchingNextPage && (
            <div className="mt-4 flex items-center justify-center py-4 text-sm text-[var(--color-text-muted)]">
              Loading more…
            </div>
          )}
          {!hasNextPage && allImages.length > 0 && (
            <div className="mt-4 flex items-center justify-center py-4 text-sm text-[var(--color-text-muted)]">
              All {total} images loaded
            </div>
          )}
        </>
      )}

      {/* Confirm delete dialog */}
      {showConfirm && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm">
          <div className="mx-4 w-full max-w-sm rounded-xl border border-[var(--color-border)] bg-[var(--color-surface)] p-6 shadow-xl">
            <h2 className="mb-2 text-lg font-semibold text-[var(--color-text-primary)]">
              Delete {selected.size} image{selected.size !== 1 ? 's' : ''}?
            </h2>
            <p className="mb-4 text-sm text-[var(--color-text-secondary)]">
              This action cannot be undone. The images will be permanently deleted from storage.
            </p>
            <div className="flex justify-end gap-3">
              <button
                onClick={() => setShowConfirm(false)}
                className="rounded-lg px-4 py-2 text-sm text-[var(--color-text-secondary)] hover:bg-[var(--color-surface-glass)]"
              >
                Cancel
              </button>
              <button
                onClick={confirmDelete}
                disabled={isDeleting}
                className="rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
              >
                {isDeleting ? 'Deleting…' : 'Delete'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
```

Key UI changes:
- **Checkbox overlay**: When `selectMode` is on, each image shows a checkbox in the top-left
- **Click behavior**: In select mode, clicking toggles selection; in normal mode, navigates to detail
- **Selection toolbar**: Appears when `selectMode` is true, shows count + "Select All"/"Delete"/"X" buttons
- **Confirmation dialog**: Modal with count, warning text, Cancel/Delete buttons
- **Selected state**: Images have accent border + ring when selected
- **Long-press to enter select mode**: Not implemented (too complex for now — click on checkbox area enters mode)

- [ ] **Step 2: Verify TypeScript and build**

```bash
cd web-ui && npx tsc --noEmit && npm run build
```

Expected: PASS

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/pages/Gallery.tsx
git commit -m "feat: add bulk selection and batch delete to Gallery"
```

---

### Task 4: Integration smoke test + spec/summary update + version bump

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
- [x] 批量管理 (multi-select, batch delete via POST /images/batch-delete, confirmation dialog)
```

- [ ] **Step 3: Update summary**

Add completion entry, bump recommended next to "OAuth 登录 或 /metrics Prometheus".

- [ ] **Step 4: Bump version**

```toml
version = "0.11.0"
```

- [ ] **Step 5: Commit**

```bash
git add docs/superpowers/specs/... .omo/summary/... Cargo.toml Cargo.lock docs/superpowers/plans/...
git commit -m "chore: update spec and summary for bulk management, bump version to 0.11.0"
```

---

## Self-Review Checklist

### 1. Spec Coverage
- ✅ Batch delete endpoint: Task 1
- ✅ Ownership/security: Task 1 (owner + admin check via `(user_id = $2 OR $3)`)
- ✅ Frontend selection mode: Task 3
- ✅ Select all / deselect: Task 3
- ✅ Confirmation dialog: Task 3
- ✅ Smoke test: Task 4

### 2. Placeholder Scan
- ✅ No "TBD", "TODO"
- ✅ All code shown inline with exact file paths
- ✅ Edge cases: empty ids (400), >100 ids (400), admin check, storage errors (best-effort)

### 3. Type Consistency
- ✅ `ids: Vec<Uuid>` backend ↔ `ids: string[]` frontend
- ✅ `BatchDeleteResult { deleted: number, failed: number }` consistent
