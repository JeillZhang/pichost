# P4-F: File Name Retention + Rename — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` (recommended) or `superpowers:executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `PATCH /api/v1/images/:id` endpoint to rename image display names, and inline rename UI on ImageDetail page.

**Architecture:** Single PATCH handler in existing `images.rs` route file, reusing `ImageRow` + `UploadResult::from_row()` for the response. Frontend uses TanStack Query `useMutation` for the rename call, with inline `<input>` replacing the static name display on click.

**Tech Stack:** Rust (Axum, sqlx, serde), TypeScript (React, TanStack Query, ky)

## Agent Worker Instructions

- **Required sub-skills**: `superpowers:subagent-driven-development`
- **Recommended execution mode**: `subagent-driven-development` — dispatch T0 and T1 as parallel subagents
- **Required verification**: `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`, `npm run build`
- **Version bump reminder**: After all tasks pass verification, bump version 0.16.2 → 0.16.3 in `Cargo.toml` (workspace) and `web-ui/package.json`

## Global Constraints

- Rust functions ≤50 lines, lines ≤120 chars
- No DB migration needed (P4-F uses existing `original_name` column)
- Renaming does NOT change Git repo paths or public URLs — display-only `original_name` field
- Validation: max 255 chars, no `/`, `\`, or null bytes, non-empty
- Response shape: `UploadResult` (same as upload/get responses)
- Follow existing error pattern: `(StatusCode, Json<serde_json::Value>)`
- Verify image ownership before update (check `user_id`)

---

### Task T0: Backend — Add PATCH /api/v1/images/:id rename endpoint

**Files:**
- Modify: `pichost-api/src/routes/images.rs`
- Modify: `pichost-api/src/main.rs`

**Interfaces:**
- Produces: `pub async fn rename_image(State, Extension<AuthUser>, Path<Uuid>, Json<UpdateImageRequest>) -> Result<Json<UploadResult>, RouteError>`
- Produces: `struct UpdateImageRequest { original_name: String }` (Deserialize)
- Consumes: `UploadResult::from_row(ImageRow)` (existing), `ImageRow` (existing, sqlx::FromRow)

**depends_on:** []
**breaking:** false

**ac:**
- given: a user-owned image with `original_name = "photo.jpg"`
  when: `PATCH /api/v1/images/:id` with `{"original_name": "renamed.png"}`
  then: returns 200 with `UploadResult` where `original_name = "renamed.png"`, `markdown = "![renamed.png](...)"`, `html` alt updated, `bbcode` unchanged (no name ref)

- given: a valid image id that belongs to a different user
  when: authenticated user A PATCHes user B's image
  then: returns 404 "image not found" (no information leak)

- given: any image id
  when: PATCH with `original_name` containing `/` or `\`
  then: returns 400 with "original_name contains invalid characters"

- given: any image id
  when: PATCH with `original_name` longer than 255 chars
  then: returns 400 with "original_name too long (max 255)"

- given: any image id
  when: PATCH with empty `original_name`
  then: returns 400 with "original_name cannot be empty"

**regression:**
- `cargo test -p pichost-api test_image_list_query_defaults -- --exact` (existing list serde must still work)
- `cargo test -p pichost-api test_move_image_request_serde -- --exact` (existing move request must still work)

**test_code:** |
```rust
#[cfg(test)]
mod rename_tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn validate_name_rejects_empty() {
        let result = validate_original_name("");
        assert!(result.is_err());
        let (code, _) = result.unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_name_rejects_too_long() {
        let long_name = "a".repeat(256);
        let result = validate_original_name(&long_name);
        assert!(result.is_err());
        let (code, _) = result.unwrap_err();
        assert_eq!(code, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn validate_name_rejects_path_separators() {
        for ch in &['/', '\\', '\0'] {
            let name = format!("bad{}name.txt", ch);
            let result = validate_original_name(&name);
            assert!(result.is_err(), "should reject char '{}'", ch);
        }
    }

    #[test]
    fn validate_name_accepts_valid() {
        let result = validate_original_name("valid-file_name (1).jpg");
        assert!(result.is_ok());
    }

    #[test]
    fn validate_name_accepts_max_length() {
        let name = "a".repeat(255);
        let result = validate_original_name(&name);
        assert!(result.is_ok());
    }
}
```

**impl_code:** |
```rust
// ── In pichost-api/src/routes/images.rs ──

/// Request body for PATCH /api/v1/images/:id
#[derive(Debug, Deserialize)]
pub struct UpdateImageRequest {
    pub original_name: String,
}

/// Validate original_name: non-empty, ≤255 chars, no path separators or null bytes.
fn validate_original_name(name: &str) -> Result<(), RouteError> {
    if name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "original_name cannot be empty"})),
        ));
    }
    if name.len() > 255 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "original_name too long (max 255)"})),
        ));
    }
    if name.contains('/') || name.contains('\\') || name.contains('\0') {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(json!({"error": "original_name contains invalid characters"})),
        ));
    }
    Ok(())
}

/// PATCH /api/v1/images/{id} — rename an image's display name
pub async fn rename_image(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateImageRequest>,
) -> Result<Json<UploadResult>, RouteError> {
    validate_original_name(&req.original_name)?;

    let updated = sqlx::query_as::<_, ImageRow>(
        "UPDATE images SET original_name = $1 \
         WHERE id = $2 AND user_id = $3 \
         RETURNING id, public_key, original_name, url, mime_type, file_size, \
                   sha256, width, height, status, thumbnail_url, webp_url, \
                   created_at, category_id, storage_config_id"
    )
    .bind(&req.original_name)
    .bind(id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await
    .map_err(|e| {
        tracing::warn!("Rename image query failed: {e}");
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({"error": "internal server error"})),
        )
    })?
    .ok_or_else(|| {
        (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "image not found"})),
        )
    })?;

    // Clear cache entry so next GET returns the updated name
    let _: Result<(), _> = state.cache.del(&format!("pichost:meta:{}", id)).await;

    Ok(Json(UploadResult::from_row(updated)))
}

// ── In pichost-api/src/main.rs, image_routes() ──
// Change:
//   .route("/{id}", get(routes::images::get_image).delete(routes::images::delete_image))
// To:
//   .route("/{id}", get(routes::images::get_image)
//       .patch(routes::images::rename_image)
//       .delete(routes::images::delete_image))
```

**verify:**
- `cargo test -p pichost-api rename_tests` (new validation tests pass)
- `cargo clippy --workspace -- -D warnings`

**migration_verify:** (none — no DB migration)

---

### Task T1: Frontend — Add renameImage API client + ImageDetail inline rename

**Files:**
- Modify: `web-ui/src/api/client.ts`
- Modify: `web-ui/src/pages/ImageDetail.tsx`

**Interfaces:**
- Produces: `export async function renameImage(id: string, originalName: string): Promise<ImageInfo>`
- Consumes: `ImageInfo` (existing type with `original_name: string`)

**depends_on:** [T0]
**breaking:** false

**ac:**
- given: ImageDetail page showing image "photo.jpg"
  when: user clicks the filename text, then types "new-name.png" and presses Enter
  then: the filename display updates to "new-name.png", server PATCH is called, TanStack Query cache is invalidated

- given: inline rename input is active
  when: user presses Escape or clicks away (blur)
  then: the input reverts back to the original name display, no PATCH is made

- given: inline rename input is active with empty value
  when: user presses Enter
  then: rename is NOT submitted (empty name rejected at input level)

- given: ImageDetail page is open
  when: network request fails (server error)
  then: toast error message appears, filename reverts to original value

**regression:**
- `npm run build` (existing build must pass)
- Gallery scroll and ImageDetail load must still work

**test_code:** |
```typescript
// ── No dedicated test file (frontend tests not in scope for this phase).
// Verification is manual/visual QA:
// 1. Navigate to ImageDetail for any image
// 2. Click the filename → input appears with current value
// 3. Type new name → Enter → observe updated name + no error toast
// 4. Click name → press Escape → observe revert to original
// 5. Click name → type invalid chars (/) → Enter → observe error toast
```

**impl_code:** |
```typescript
// ── In web-ui/src/api/client.ts ──

export async function renameImage(id: string, originalName: string): Promise<ImageInfo> {
  return api.patch(`images/${id}`, { json: { original_name: originalName } }).json<ImageInfo>()
}
```

```tsx
// ── In web-ui/src/pages/ImageDetail.tsx ──
// Add imports:
//   import { Pencil } from 'lucide-react'
//   import { renameImage } from '../api/client'
//
// Add state variables inside ImageDetail component:
//   const [isRenaming, setIsRenaming] = useState(false)
//   const [renameValue, setRenameValue] = useState('')
//
// Add useMutation:
//   const renameMutation = useMutation({
//     mutationFn: ({ imageId, name }: { imageId: string; name: string }) =>
//       renameImage(imageId, name),
//     onSuccess: () => {
//       queryClient.invalidateQueries({ queryKey: ['image', id] })
//       queryClient.invalidateQueries({ queryKey: ['images'] })
//       setIsRenaming(false)
//     },
//     onError: (e: unknown) => {
//       const msg = e instanceof Error ? e.message : 'Rename failed'
//       toast.error(msg)
//       setIsRenaming(false)
//     },
//   })

// Replace lines 127-129 (the Name: display) with:

{/* File name with inline rename */}
<div className="flex items-center gap-2">
  <span className="text-[var(--color-text-secondary)]">Name:</span>
  {isRenaming ? (
    <input
      autoFocus
      type="text"
      value={renameValue}
      onChange={(e) => setRenameValue(e.target.value)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' && renameValue.trim()) {
          renameMutation.mutate({ imageId: id!, name: renameValue.trim() })
        } else if (e.key === 'Escape') {
          setIsRenaming(false)
        }
      }}
      onBlur={() => setIsRenaming(false)}
      disabled={renameMutation.isPending}
      className="flex-1 rounded border border-[var(--color-border)] bg-[var(--color-surface)] px-2 py-0.5 text-sm text-[var(--color-text-primary)] focus:border-[var(--color-accent)] focus:outline-none"
    />
  ) : (
    <button
      onClick={() => {
        setRenameValue(img.original_name)
        setIsRenaming(true)
      }}
      className="group flex items-center gap-1 text-[var(--color-text-primary)] hover:text-[var(--color-accent)]"
    >
      <span>{img.original_name}</span>
      <Pencil className="h-3 w-3 opacity-0 group-hover:opacity-100 transition-opacity" />
    </button>
  )}
  {renameMutation.isPending && (
    <span className="text-xs text-[var(--color-text-muted)]">Saving...</span>
  )}
</div>
```

**verify:**
- `npx tsc --noEmit` (TypeScript check)
- `npm run build`
