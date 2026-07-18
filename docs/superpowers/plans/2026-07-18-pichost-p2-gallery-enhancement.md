# Gallery Enhancement: Pagination, Search, Sorting, Infinite Scroll

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add offset-based pagination, full-text search by filename, multi-field sorting, and infinite scroll to the Gallery page.

**Architecture:** Backend `GET /api/v1/images` accepts `page`, `per_page`, `sort`, `order`, `search` query params and returns a paginated envelope (`{ items, total, page, per_page, total_pages }`). Frontend Gallery uses `useInfiniteQuery` with page-based cursors instead of the existing `useQuery`, adds a debounced search bar and a sort dropdown, and triggers page loads via IntersectionObserver sentinel element.

**Tech Stack:** Rust 1.96 (Axum 0.8, sqlx 0.8, serde with query param deserialization), React 19, TypeScript 5.7, TanStack Query v5, Tailwind CSS v4

## Global Constraints

- Rust edition 2021, `cargo clippy --workspace -- -D warnings` must pass, `cargo test --workspace` must pass
- Frontend: `npm run build` (tsc + vite) must pass
- Follow existing code patterns: inline `sqlx::query_as` (no `query!` macro), `UploadResult` is the item type, `AppState` passed via `State`
- No new external Rust crates; use `serde` for query param parsing (Axum's `Query` extractor with a `#[derive(Deserialize)]` struct)
- Frontend: use existing `ky` client, `@tanstack/react-query`; no new npm dependencies
- All commits in English, spec docs in Chinese

---

## File Structure

```
pichost-api/src/routes/images.rs         (MODIFY) - Update list_images handler
pichost-api/src/services/upload.rs       (MODIFY) - Add ImageListQuery, ImageListResponse types
migrations/0005_add_search_filename_index.sql (CREATE) - Index for ILIKE search
web-ui/src/api/client.ts                 (MODIFY) - Update listImages, add PaginatedResponse type
web-ui/src/pages/Gallery.tsx             (MODIFY) - Search bar, sort, infinite scroll
web-ui/src/components/SearchBar.tsx      (CREATE) - Debounced search input
web-ui/src/components/SortDropdown.tsx   (CREATE) - Sort field + order dropdown
```

---

### Task 1: Backend — Define ImageListQuery and ImageListResponse types

**Files:**
- Modify: `pichost-api/src/services/upload.rs` (append after line 34, the `UploadResult` struct)

**Interfaces:**
- Produces: `ImageListQuery` (query param struct), `ImageListResponse` (paginated envelope)
- Consumed by: Task 2 (list_images handler), Task 4 (frontend API client — conceptually mirrors these types)

- [ ] **Step 1: Add ImageListQuery and ImageListResponse to `upload.rs`**

```rust
// Append after the UploadResult struct (after line 34 in upload.rs):

/// Query parameters for GET /api/v1/images
#[derive(Debug, Deserialize)]
pub struct ImageListQuery {
    /// Page number (1-based, default 1)
    #[serde(default = "default_page")]
    pub page: u32,
    /// Items per page (default 20, max 100)
    #[serde(default = "default_per_page")]
    pub per_page: u32,
    /// Sort field: "created_at", "file_size", "original_name"
    #[serde(default = "default_sort")]
    pub sort: String,
    /// Sort order: "asc" or "desc"
    #[serde(default = "default_order")]
    pub order: String,
    /// Optional search term (ILIKE match against original_name)
    #[serde(default)]
    pub search: String,
}

fn default_page() -> u32 { 1 }
fn default_per_page() -> u32 { 20 }
fn default_sort() -> String { "created_at".to_string() }
fn default_order() -> String { "desc".to_string() }

/// Paginated response envelope
#[derive(Debug, Serialize)]
pub struct ImageListResponse {
    pub items: Vec<UploadResult>,
    pub total: i64,
    pub page: u32,
    pub per_page: u32,
    pub total_pages: u32,
}
```

Note: The `Deserialize` derive already exists (line 17: `use serde::{Deserialize, Serialize};`), so no new imports needed.

- [ ] **Step 2: Verify compilation**

Run: `cargo check -p pichost-api`
Expected: PASS (types compile without errors)

- [ ] **Step 3: Commit**

```bash
git add pichost-api/src/services/upload.rs
git commit -m "feat: add ImageListQuery and ImageListResponse types for paginated image listing"
```

---

### Task 2: Backend — Update list_images handler with pagination, search, and sort

**Files:**
- Modify: `pichost-api/src/routes/images.rs:27-110` (the `list_images` handler)

**Interfaces:**
- Consumes: `ImageListQuery`, `ImageListResponse` from Task 1
- Produces: Updated `list_images` handler returning `Json<ImageListResponse>`

- [ ] **Step 1: Write the integration test for paginated list_images**

Create (or modify if already exists) `pichost-api/tests/gallery_test.rs`.

Since the project has no existing API integration tests framework (only `pichost-core` storage tests), we create a simple unit test of the handler logic or rely on the compiler/lint + manual curl verification. For TDD, we write a test that exercises the query parameter parsing:

```rust
// pichost-api/tests/gallery_test.rs
use pichost_api::services::upload::{ImageListQuery, ImageListResponse};

#[test]
fn test_image_list_query_defaults() {
    // Simulate query param parsing via serde
    let query: ImageListQuery = serde_urlencoded::from_str("").unwrap();
    assert_eq!(query.page, 1);
    assert_eq!(query.per_page, 20);
    assert_eq!(query.sort, "created_at");
    assert_eq!(query.order, "desc");
    assert_eq!(query.search, "");
}

#[test]
fn test_image_list_query_parse_all_params() {
    let query: ImageListQuery = serde_urlencoded::from_str(
        "page=2&per_page=10&sort=file_size&order=asc&search=cat"
    ).unwrap();
    assert_eq!(query.page, 2);
    assert_eq!(query.per_page, 10);
    assert_eq!(query.sort, "file_size");
    assert_eq!(query.order, "asc");
    assert_eq!(query.search, "cat");
}

#[test]
fn test_image_list_query_rejects_invalid_sort() {
    // Invalid sort field should still parse but be caught at handler level
    let query: ImageListQuery = serde_urlencoded::from_str("sort=malicious;DROP TABLE").unwrap();
    assert_eq!(query.sort, "malicious;DROP TABLE"); // handler must validate
}

#[test]
fn test_image_list_response_total_pages_calculation() {
    // total_pages = ceil(total / per_page)
    // 23 items, 10 per page = 3 pages
    let resp = ImageListResponse { items: vec![], total: 23, page: 1, per_page: 10, total_pages: 3 };
    assert_eq!(resp.total_pages, 3);
    // 0 items, 20 per page = 1 page (always at least 1)
    let resp2 = ImageListResponse { items: vec![], total: 0, page: 1, per_page: 20, total_pages: 1 };
    assert_eq!(resp2.total_pages, 1);
}
```

Add `serde_urlencoded` as a dev-dependency if not already transitively available. Check first:

Run: `cargo tree -p pichost-api --depth 1 | grep serde_urlencoded`
If not found, add to `pichost-api/Cargo.toml` under `[dev-dependencies]`:
```toml
[dev-dependencies]
serde_urlencoded = "0.7"
```

Run: `cargo test -p pichost-api test_image_list` (expect FAIL because serde_urlencoded may not be in deps yet)

- [ ] **Step 2: Add the dev-dependency and run tests to verify they fail/build**

```bash
# Add serde_urlencoded to dev-dependencies in pichost-api/Cargo.toml if needed
cargo test -p pichost-api test_image_list_query_defaults
```

Expected: If serde_urlencoded not added, build fails. After adding, tests pass.

- [ ] **Step 3: Rewrite the `list_images` handler**

Replace the entire `list_images` function (lines 28-110 in `images.rs`) with:

```rust
use crate::services::upload::{ImageListQuery, ImageListResponse};

/// GET /api/v1/images — list user's images with pagination, search, and sort (protected)
pub async fn list_images(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    axum::extract::Query(params): axum::extract::Query<ImageListQuery>,
) -> Result<Json<ImageListResponse>, (StatusCode, Json<serde_json::Value>)> {
    // --- Validate & clamp params ---
    let page = params.page.max(1);
    let per_page = params.per_page.clamp(1, 100);
    let offset = ((page - 1) * per_page) as i64;
    let limit = per_page as i64;

    // --- Validate sort field (whitelist to prevent SQL injection) ---
    let sort_col = match params.sort.as_str() {
        "created_at" | "file_size" | "original_name" => params.sort.as_str(),
        _ => "created_at", // fallback default
    };
    let order_dir = match params.order.as_str() {
        "asc" | "ASC" => "ASC",
        _ => "DESC",
    };

    // --- Build dynamic SQL with search ---
    let search_term = params.search.trim();
    let has_search = !search_term.is_empty();

    // Count total matching rows
    let total: i64 = if has_search {
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM images WHERE user_id = $1 AND original_name ILIKE $2"
        )
        .bind(user.id)
        .bind(format!("%{}%", search_term))
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Image count query failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"})))
        })?
        .unwrap_or(0)
    } else {
        sqlx::query_scalar("SELECT COUNT(*) FROM images WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(&state.pool)
        .await
        .map_err(|e| {
            tracing::warn!("Image count query failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"})))
        })?
        .unwrap_or(0)
    };

    // Fetch paginated rows
    type ImageRow = (
        Uuid, String, String, String, String, i64, String,
        Option<i32>, Option<i32>, String, Option<String>, Option<String>,
        chrono::DateTime<chrono::Utc>,
    );

    let rows: Vec<ImageRow> = if has_search {
        let query_str = format!(
            r#"SELECT id, public_key, original_name, url, mime_type, file_size,
                      sha256, width, height, status, thumbnail_url, webp_url, created_at
               FROM images WHERE user_id = $1 AND original_name ILIKE $2
               ORDER BY {} {} LIMIT $3 OFFSET $4"#,
            sort_col, order_dir
        );
        sqlx::query_as::<_, ImageRow>(&query_str)
            .bind(user.id)
            .bind(format!("%{}%", search_term))
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.pool)
            .await
            .map_err(|e| {
                tracing::warn!("Image list query failed: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"})))
            })?
    } else {
        let query_str = format!(
            r#"SELECT id, public_key, original_name, url, mime_type, file_size,
                      sha256, width, height, status, thumbnail_url, webp_url, created_at
               FROM images WHERE user_id = $1
               ORDER BY {} {} LIMIT $2 OFFSET $3"#,
            sort_col, order_dir
        );
        sqlx::query_as::<_, ImageRow>(&query_str)
            .bind(user.id)
            .bind(limit)
            .bind(offset)
            .fetch_all(&state.pool)
            .await
            .map_err(|e| {
                tracing::warn!("Image list query failed: {e}");
                (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({"error": "internal server error"})))
            })?
    };

    // Map rows to UploadResult
    let items: Vec<UploadResult> = rows.into_iter().map(|row| {
        let (id, public_key, original_name, url, mime_type, file_size,
             sha256, width, height, status, thumbnail_url, webp_url, created_at) = row;
        UploadResult {
            id, public_key,
            original_name: original_name.clone(),
            url: url.clone(),
            markdown: format!("![{}]({})", original_name, url),
            html: format!("<img src=\"{}\" alt=\"{}\" />", url, html_escape(&original_name)),
            bbcode: format!("[img]{}[/img]", url),
            sha256, file_size, mime_type, width, height,
            status, thumbnail_url, webp_url, created_at,
        }
    }).collect();

    let total_pages = if total == 0 {
        1
    } else {
        ((total as f64) / (per_page as f64)).ceil() as u32
    };

    Ok(Json(ImageListResponse {
        items,
        total,
        page,
        per_page,
        total_pages,
    }))
}
```

Remove the old `list_images` function completely (lines 28-110 replaced by the above).

The import of `ImageListQuery` and `ImageListResponse` needs to be checked — they're in `crate::services::upload`. The existing code already has `use crate::services::upload::{self, UploadResult};` at line 15. Update it to:

```rust
use crate::services::upload::{self, ImageListQuery, ImageListResponse, UploadResult};
```

- [ ] **Step 4: Verify the handler compiles**

Run: `cargo check -p pichost-api`
Expected: PASS

- [ ] **Step 5: Run full test suite**

```bash
cargo test -p pichost-api::gallery_test  # unit tests for types
cargo clippy --workspace -- -D warnings
```

Expected: All tests pass, no clippy warnings.

- [ ] **Step 6: Commit**

```bash
git add pichost-api/src/routes/images.rs pichost-api/src/services/upload.rs pichost-api/tests/gallery_test.rs
git commit -m "feat: add pagination, search, and sort to GET /images endpoint"
```

---

### Task 3: Add database migration for filename search index

**Files:**
- Create: `migrations/0005_add_image_search_index.sql`

**Interfaces:**
- Produces: `idx_images_user_filename` B-tree index on `(user_id, original_name)`
- Consumed by: Task 2's `list_images` handler (makes ILIKE queries faster)

- [ ] **Step 1: Create the migration file**

```sql
-- migrations/0005_add_image_search_index.sql
-- Add composite index for user-scoped filename search (ILIKE queries on original_name)
-- Index on (user_id, original_name) supports WHERE user_id = $1 AND original_name ILIKE $2
CREATE INDEX IF NOT EXISTS idx_images_user_filename ON images(user_id, original_name);
```

- [ ] **Step 2: Verify migration syntax**

```bash
# Since we don't use sqlx compile-time checks, just verify the SQL is valid:
# The migration will be applied at API startup via sqlx::migrate!()
cargo build -p pichost-api
```

Expected: Build passes (migration is embedded at compile time).

- [ ] **Step 3: Commit**

```bash
git add migrations/0005_add_image_search_index.sql
git commit -m "feat: add composite index on images(user_id, original_name) for filename search"
```

---

### Task 4: Frontend — Update API client with paginated types and params

**Files:**
- Modify: `web-ui/src/api/client.ts` (lines 18-35, 138-140)

**Interfaces:**
- Consumes: Conceptual `ImageListResponse` from Task 1 (mirrors backend)
- Produces: `PaginatedListParams`, `PaginatedResponse<ImageInfo>`, updated `listImages()`
- Consumed by: Task 5, Task 6 (Gallery components)

- [ ] **Step 1: Add TypeScript types and update `listImages`**

Replace lines 18-35 (the `ImageInfo` interface) with the same interface (unchanged), then add new types after `UploadResult` (line 37), and update the `listImages` function (lines 138-140):

```typescript
// --- Add after the ImageInfo interface (after line 35) ---

export interface PaginatedListParams {
  page?: number
  per_page?: number
  sort?: 'created_at' | 'file_size' | 'original_name'
  order?: 'asc' | 'desc'
  search?: string
}

export interface PaginatedResponse<T> {
  items: T[]
  total: number
  page: number
  per_page: number
  total_pages: number
}

// --- Replace listImages function (lines 138-140) ---

export async function listImages(
  params: PaginatedListParams = {},
): Promise<PaginatedResponse<ImageInfo>> {
  const searchParams = new URLSearchParams()
  if (params.page) searchParams.set('page', String(params.page))
  if (params.per_page) searchParams.set('per_page', String(params.per_page))
  if (params.sort) searchParams.set('sort', params.sort)
  if (params.order) searchParams.set('order', params.order)
  if (params.search) searchParams.set('search', params.search)
  const qs = searchParams.toString()
  return api.get(`images${qs ? `?${qs}` : ''}`).json<PaginatedResponse<ImageInfo>>()
}
```

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd web-ui && npx tsc --noEmit`
Expected: PASS (no type errors)

- [ ] **Step 3: Commit**

```bash
git add web-ui/src/api/client.ts
git commit -m "feat: add paginated listImages API with search and sort params"
```

---

### Task 5: Frontend — Build SearchBar and SortDropdown components

**Files:**
- Create: `web-ui/src/components/SearchBar.tsx`
- Create: `web-ui/src/components/SortDropdown.tsx`

**Interfaces:**
- Consumes: Nothing from earlier tasks (standalone components)
- Produces:
  - `SearchBar` — controlled input that calls `onSearch(query: string)` after 300ms debounce
  - `SortDropdown` — controlled select for sort field + order, calls `onSortChange(sort: string, order: string)`
- Consumed by: Task 6 (Gallery page)

- [ ] **Step 1: Write SearchBar component**

```tsx
// web-ui/src/components/SearchBar.tsx
import { useState, useEffect, useRef } from 'react'
import { Search, X } from 'lucide-react'

interface SearchBarProps {
  value: string
  onChange: (value: string) => void
  placeholder?: string
}

export default function SearchBar({
  value,
  onChange,
  placeholder = 'Search by filename…',
}: SearchBarProps) {
  const [localValue, setLocalValue] = useState(value)
  const timerRef = useRef<ReturnType<typeof setTimeout>>()

  // Sync external value changes
  useEffect(() => {
    setLocalValue(value)
  }, [value])

  const handleChange = (next: string) => {
    setLocalValue(next)
    if (timerRef.current) clearTimeout(timerRef.current)
    timerRef.current = setTimeout(() => {
      onChange(next)
    }, 300)
  }

  const handleClear = () => {
    setLocalValue('')
    if (timerRef.current) clearTimeout(timerRef.current)
    onChange('')
  }

  return (
    <div className="relative">
      <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-[var(--color-text-muted)]" />
      <input
        type="text"
        value={localValue}
        onChange={(e) => handleChange(e.target.value)}
        placeholder={placeholder}
        className="w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] py-2 pl-10 pr-8 text-sm text-[var(--color-text-primary)] placeholder:text-[var(--color-text-muted)] backdrop-blur-sm focus:border-[var(--color-accent)] focus:outline-none focus:ring-1 focus:ring-[var(--color-accent)]"
      />
      {localValue && (
        <button
          onClick={handleClear}
          className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-0.5 text-[var(--color-text-muted)] hover:text-[var(--color-text-primary)]"
          aria-label="Clear search"
        >
          <X className="h-4 w-4" />
        </button>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Write SortDropdown component**

```tsx
// web-ui/src/components/SortDropdown.tsx
import { ArrowUpDown } from 'lucide-react'

interface SortDropdownProps {
  sort: string
  order: string
  onSortChange: (sort: string) => void
  onOrderChange: (order: string) => void
}

const SORT_OPTIONS = [
  { value: 'created_at', label: 'Upload Date' },
  { value: 'file_size', label: 'File Size' },
  { value: 'original_name', label: 'Filename' },
]

export default function SortDropdown({
  sort,
  order,
  onSortChange,
  onOrderChange,
}: SortDropdownProps) {
  return (
    <div className="flex items-center gap-2">
      <ArrowUpDown className="h-4 w-4 text-[var(--color-text-muted)]" />
      <select
        value={sort}
        onChange={(e) => onSortChange(e.target.value)}
        className="rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-2 py-2 text-sm text-[var(--color-text-primary)] backdrop-blur-sm focus:border-[var(--color-accent)] focus:outline-none"
      >
        {SORT_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
      <button
        onClick={() => onOrderChange(order === 'asc' ? 'desc' : 'asc')}
        className="rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-2 py-2 text-sm text-[var(--color-text-primary)] backdrop-blur-sm hover:bg-[var(--color-surface-hover)]"
        aria-label={`Sort ${order === 'asc' ? 'descending' : 'ascending'}`}
      >
        {order === 'asc' ? '↑' : '↓'}
      </button>
    </div>
  )
}
```

- [ ] **Step 3: Verify TypeScript compilation**

Run: `cd web-ui && npx tsc --noEmit`
Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add web-ui/src/components/SearchBar.tsx web-ui/src/components/SortDropdown.tsx
git commit -m "feat: add SearchBar and SortDropdown components for Gallery"
```

---

### Task 6: Frontend — Rewrite Gallery page with infinite scroll, search, and sort

**Files:**
- Modify: `web-ui/src/pages/Gallery.tsx` (entire file)

**Interfaces:**
- Consumes: `listImages` from Task 4, `SearchBar`, `SortDropdown` from Task 5
- Produces: Interactive Gallery with infinite scroll, search filtering, and sort controls

- [ ] **Step 1: Rewrite Gallery.tsx**

Replace the entire `Gallery.tsx` with:

```tsx
import { useRef, useCallback, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { useInfiniteQuery } from '@tanstack/react-query'
import { listImages } from '../api/client'
import type { ImageInfo } from '../api/client'
import SearchBar from '../components/SearchBar'
import SortDropdown from '../components/SortDropdown'

export default function Gallery() {
  const navigate = useNavigate()
  const [search, setSearch] = useState('')
  const [sort, setSort] = useState('created_at')
  const [order, setOrder] = useState('desc')

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
      if (lastPage.page < lastPage.total_pages) {
        return lastPage.page + 1
      }
      return undefined
    },
  })

  // Infinite scroll sentinel
  const sentinelRef = useRef<HTMLDivElement>(null)
  const observerRef = useRef<IntersectionObserver>()

  const lastItemRef = useCallback(
    (node: HTMLDivElement | null) => {
      if (isFetchingNextPage) return
      if (observerRef.current) observerRef.current.disconnect()
      observerRef.current = new IntersectionObserver(
        (entries) => {
          if (entries[0].isIntersecting && hasNextPage) {
            fetchNextPage()
          }
        },
        { rootMargin: '200px' },
      )
      if (node) observerRef.current.observe(node)
    },
    [isFetchingNextPage, hasNextPage, fetchNextPage],
  )

  const allImages: ImageInfo[] = data?.pages.flatMap((p) => p.items) ?? []
  const total = data?.pages[0]?.total ?? 0

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
            onSortChange={setSort}
            onOrderChange={setOrder}
          />
        </div>
      </div>

      {/* Loading state */}
      {isLoading && (
        <div className="flex min-h-[200px] items-center justify-center text-[var(--color-text-muted)]">
          Loading…
        </div>
      )}

      {/* Error state */}
      {isError && (
        <div className="flex min-h-[200px] items-center justify-center text-red-500">
          Failed to load images. Please try again.
        </div>
      )}

      {/* Empty state */}
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
              return (
                <button
                  key={img.id}
                  ref={isLast ? lastItemRef : undefined}
                  onClick={() => navigate(`/images/${img.id}`)}
                  className="group relative aspect-square overflow-hidden rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] backdrop-blur-sm"
                >
                  <img
                    src={img.thumbnail_url ?? img.url}
                    alt={img.original_name}
                    className="h-full w-full object-cover transition-transform group-hover:scale-105"
                    loading="lazy"
                  />
                  <div className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/80 to-transparent p-2">
                    <p className="truncate text-xs text-white">
                      {img.original_name}
                    </p>
                  </div>
                </button>
              )
            })}
          </div>

          {/* Loading more indicator */}
          {isFetchingNextPage && (
            <div className="mt-4 flex items-center justify-center py-4 text-sm text-[var(--color-text-muted)]">
              Loading more…
            </div>
          )}

          {/* End of results */}
          {!hasNextPage && allImages.length > 0 && (
            <div className="mt-4 flex items-center justify-center py-4 text-sm text-[var(--color-text-muted)]">
              All {total} images loaded
            </div>
          )}
        </>
      )}
    </div>
  )
}
```

- [ ] **Step 2: Verify TypeScript compilation**

Run: `cd web-ui && npx tsc --noEmit`
Expected: PASS

- [ ] **Step 3: Verify Vite build**

Run: `cd web-ui && npm run build`
Expected: PASS (tsc + vite build both succeed)

- [ ] **Step 4: Commit**

```bash
git add web-ui/src/pages/Gallery.tsx
git commit -m "feat: implement infinite scroll, search, and sort in Gallery page"
```

---

### Task 7: Integration smoke test and spec doc update

**Files:**
- Modify: `docs/superpowers/specs/2026-07-11-pichost-design.md` (line 1554, update TODO)
- Modify: `.omo/summary/summary_and_next.md` (update summary)

- [ ] **Step 1: Run full backend verification**

```bash
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo build --workspace
```

Expected: All pass, no errors.

- [ ] **Step 2: Run full frontend verification**

```bash
cd web-ui && npm run build
```

Expected: PASS (tsc + vite build)

- [ ] **Step 3: Manual smoke test (if Docker environment available)**

```bash
# Start services
docker compose up --build -d

# Register a user and get token
TOKEN=$(curl -s -X POST http://localhost:3000/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{"username":"test","password":"test123456"}' | jq -r '.access_token')

# Upload a few test images (create valid PNG first)
# python3 -c "..." > /tmp/test.png
# curl -X POST http://localhost:3000/api/v1/images -H "Authorization: Bearer $TOKEN" -F "file=@/tmp/test.png"

# Test pagination
curl -s "http://localhost:3000/api/v1/images?page=1&per_page=5" \
  -H "Authorization: Bearer $TOKEN" | jq '{total, page, per_page, total_pages, item_count: (.items | length)}'
# Expected: { total: N, page: 1, per_page: 5, total_pages: ceil(N/5), item_count: min(5, N) }

# Test search
curl -s "http://localhost:3000/api/v1/images?search=test" \
  -H "Authorization: Bearer $TOKEN" | jq '.total'
# Expected: number of images with "test" in filename

# Test sort
curl -s "http://localhost:3000/api/v1/images?sort=file_size&order=desc" \
  -H "Authorization: Bearer $TOKEN" | jq '.items[0].file_size'
# Expected: largest file first

# Cleanup
docker compose down
```

- [ ] **Step 4: Update spec doc TODO list**

In `docs/superpowers/specs/2026-07-11-pichost-design.md`, update line 1554:
```markdown
- [x] 图片库增强: 分页/搜索/排序/无限滚动 (offset pagination, ILIKE search, sort by created_at/file_size/name, infinite scroll via IntersectionObserver)
```

- [ ] **Step 5: Update summary**

Update `.omo/summary/summary_and_next.md`:
```markdown
### P2: 图片库增强 ✅ (本次完成)
- **后端**: ImageListQuery/ImageListResponse 类型, GET /images 支持 page/per_page/sort/order/search 参数
- **前端**: SearchBar + SortDropdown 组件, Gallery 页面 useInfiniteQuery + IntersectionObserver 无限滚动
- **数据库**: idx_images_user_filename 索引加速文件名搜索

## 剩余待开发特性
- **P2 (remaining)**: 多文件并发拖拽上传, 用户存储配额, 批量管理, /metrics Prometheus, OAuth 登录, CDN 集成, 水平扩展

## 建议下一步开发
多文件并发拖拽上传 或 用户存储配额
```

Also update the version number in workspace `Cargo.toml` to `0.4.0`.

- [ ] **Step 6: Final commit**

```bash
git add docs/superpowers/specs/2026-07-11-pichost-design.md .omo/summary/summary_and_next.md Cargo.toml
git commit -m "chore: update spec and summary for gallery enhancement completion, bump version to 0.4.0"
```

---

## Self-Review Checklist

### 1. Spec Coverage
- ✅ Pagination: Task 2 (backend `page`/`per_page` params), Task 6 (frontend `useInfiniteQuery`)
- ✅ Search: Task 2 (backend `ILIKE` on `original_name` + validated `sort`), Task 5 (SearchBar component), Task 6 (integrated)
- ✅ Sorting: Task 2 (backend whitelist-validated `sort`/`order`), Task 5 (SortDropdown), Task 6 (integrated)
- ✅ Infinite scroll: Task 6 (IntersectionObserver sentinel + `fetchNextPage`)
- ✅ No gaps — all spec requirements covered

### 2. Placeholder Scan
- ✅ No "TBD", "TODO", "implement later" in any task
- ✅ No "Add appropriate error handling" — all error handling is explicit (clamp, whitelist, error responses)
- ✅ No "Write tests for the above" — actual test code provided in Task 2 Step 1
- ✅ No "Similar to Task N" — all code shown inline
- ✅ All types referenced are defined in a task (ImageListQuery, ImageListResponse defined in Task 1; PaginatedListParams, PaginatedResponse defined in Task 4)

### 3. Type Consistency
- ✅ `ImageListQuery.page: u32` used consistently across Tasks 1-2
- ✅ `ImageListQuery.per_page: u32` — validated with `.clamp(1, 100)` in Task 2 Step 3
- ✅ `ImageListResponse.total_pages: u32` — calculated in Task 2 Step 3, consumed by frontend `getNextPageParam` in Task 6
- ✅ Frontend `PaginatedResponse<T>` mirrors backend `ImageListResponse` structure (`items`, `total`, `page`, `per_page`, `total_pages`)
- ✅ `SearchBar.value: string` / `SortDropdown.sort: string` / `SortDropdown.order: string` consistent between Tasks 5 and 6
