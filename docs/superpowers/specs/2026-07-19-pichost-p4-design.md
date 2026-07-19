# PicHost P4 — New Feature Design

> **Status**: Design / Not Implemented
> **Version**: Target v0.15.0+
> **Date**: 2026-07-19
> **Prerequisite**: P3 (gap-fix phase) must be completed before any P4 work begins.

## Table of Contents

1. [Overview](#1-overview)
2. [P4-A: Git Storage Backends + Multi-Backend Upload](#2-p4-a-git-storage-backends--multi-backend-upload)
3. [P4-B: Clipboard Paste + URL Upload](#3-p4-b-clipboard-paste--url-upload)
4. [P4-C: Gallery Categories/Directories](#4-p4-c-gallery-categoriesdirectories)
5. [P4-D: Watermarking (Server-Side)](#5-p4-d-watermarking-server-side)
6. [P4-E: Image Preprocessing (Client-Side)](#6-p4-e-image-preprocessing-client-side)
7. [P4-F: Filename Preservation + Rename](#7-p4-f-filename-preservation--rename)
8. [Migration Plan](#8-migration-plan)
9. [Risk Assessment](#9-risk-assessment)

---

## 1. Overview

P4 introduces 6 new feature areas grouped into independent phases. Each phase can be developed and deployed independently after P4-A is complete.

| Phase | Theme | Priority | Dependencies |
|-------|-------|----------|--------------|
| **P4-A** | Git storage backends + multi-backend upload selection + Gallery filter | **Highest** | P3 complete |
| **P4-B** | Clipboard paste + URL upload | High | P4-A (shares upload pipeline) |
| **P4-C** | Gallery categories/directories | High | None |
| **P4-D** | Server-side watermarking | Medium | None |
| **P4-E** | Client-side image preprocessing | Medium | None |
| **P4-F** | Filename preservation + rename | **Low** | None |

**Phases B–F are independent of each other** and can be developed in parallel after P4-A is complete.

---

## 2. P4-A: Git Storage Backends + Multi-Backend Upload

### 2.1 Scope

- GitHub and GitCode as image storage backends via their Contents REST APIs
- Users bring their own repository + Personal Access Token (PAT)
- Multiple storage configurations per user (up to 5)
- Per-upload backend selection (max 2 simultaneously, one must be `local`)
- Gallery filtering by storage backend
- Worker (thumbnail/WebP) writes to Git backends transparently

### 2.2 Architecture Decision: API-Only Git Operations

Git operations use direct HTTP API calls (GitHub Contents API / GitCode Contents API) rather than clone-commit-push workflows. Rationale:

| Approach | Pros | Cons |
|----------|------|------|
| **API direct** | Lightweight, no local clone, fast, handles individual files | Subject to rate limits, 20MB cap on GitCode |
| **Clone + commit + push** | No size limit, bulk operations | Requires local clone dirs, slow, complex concurrency |

**Chosen**: API direct — matches PicHost's stateless architecture and single-file upload pattern.

### 2.3 Database Schema

#### New Table: `user_storage_configs`

```sql
-- Migration 0008
CREATE TABLE user_storage_configs (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        VARCHAR(64) NOT NULL,
    provider    VARCHAR(16) NOT NULL,          -- 'github' | 'gitcode' | 'local'
    is_default  BOOLEAN NOT NULL DEFAULT false,
    config      JSONB NOT NULL,                 -- provider-specific, token encrypted
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(user_id, name)
);

-- Ensure at most one default per user
CREATE UNIQUE INDEX idx_default_per_user
    ON user_storage_configs(user_id) WHERE is_default = true;
```

#### Column Added to `images`

```sql
-- Migration 0008 (continued)
ALTER TABLE images
    ADD COLUMN storage_config_id UUID
    REFERENCES user_storage_configs(id);
```

#### `config` JSONB Structure

**GitHub:**
```json
{
    "token_encrypted": "<AES-256-GCM ciphertext, base64>",
    "repo": "owner/repo",
    "branch": "main",
    "path_prefix": "pichost"
}
```

**GitCode:** Same structure, but `token_encrypted` is for GitCode PAT.

**Local:** `{}`

#### Encryption Key

New env var:

| Variable | Required | Purpose |
|----------|----------|---------|
| `PICHOST_AUTH_TOKEN_ENCRYPTION_KEY` | Yes (if Git backends enabled) | AES-256-GCM key for encrypting user-supplied PATs. Must be 32 bytes (base64 or hex encoded). Independent from `PICHOST_AUTH_JWT_SECRET`. |

Token lifecycle:
1. User submits PAT in plaintext via `POST /api/v1/users/me/storage-configs`
2. Server encrypts with AES-256-GCM before `INSERT`
3. `GitStorage::new()` decrypts at startup/runtime
4. GET endpoints return `token_masked` (e.g., `ghp_****abcd`)
5. Plaintext token never leaves server memory, never logged

### 2.4 Rust Model

```rust
// pichost-core/src/models.rs

/// A user's storage backend configuration.
pub struct UserStorageConfig {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub provider: String,       // "github" | "gitcode" | "local"
    pub is_default: bool,
    pub config: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Deserialized form of `config` for Git providers.
pub struct GitConfigDetail {
    pub token_encrypted: String,
    pub repo: String,
    pub branch: String,
    pub path_prefix: Option<String>,
}
```

### 2.5 GitStorage Implementation

**File**: `pichost-core/src/storage/git.rs`

One struct handles both GitHub and GitCode via a `GitProvider` enum.

```rust
pub enum GitProvider { GitHub, GitCode }

pub struct GitStorage {
    provider: GitProvider,
    client: reqwest::Client,
    owner: String,              // parsed from "owner/repo"
    repo: String,
    branch: String,
    path_prefix: Option<String>,
    token: String,              // decrypted at creation time
    raw_base_url: String,       // "raw.githubusercontent.com" or "raw.gitcode.com"
    api_base_url: String,       // "https://api.github.com" or "https://api.gitcode.com/api/v5"
}
```

#### Trait Methods

| Method | Implementation |
|--------|---------------|
| `backend_name()` | Returns `"github"` or `"gitcode"` based on `provider` |
| `put(key, data, mime)` | `PUT` (GitHub) or `POST` (GitCode) to `.../contents/{path}` with Base64-encoded content + commit message + branch. Falls back to `multipart/file_upload` endpoint for GitCode files >20MB. Returns raw public URL. |
| `get(key)` | `GET .../raw/{path}?ref={branch}` → raw bytes |
| `delete(key)` | `GET .../contents/{path}` to obtain SHA → `DELETE .../contents/{path}` |
| `exists(key)` | `GET .../contents/{path}` → 200 = exists, 404 = not found |
| `public_url(key)` | `https://{raw_base_url}/{owner}/{repo}/{branch}/{full_path}` |

#### File Path Convention

```
{path_prefix}/{YYYY}/{MM}/{DD}/{public_key}.{ext}
```

Example: `pichost/2026/07/19/a3f8c2.png`

- `path_prefix` defaults to `"pichost"` if user provides none
- Extension derived from `content_type` (MIME → ext mapping)
- Date from server clock at upload time
- **`storage_key` in DB stores the full path** (e.g., `pichost/2026/07/19/a3f8c2.png`) for Git backends. This differs from the current local storage format (`{user_id}/{public_key}`) — each backend owns its `storage_key` format. The `StorageBackend::put()` receives a simplified key and returns the full storage key to be persisted.

#### Rate Limiting

| Provider | Limit | Handling |
|----------|-------|----------|
| GitHub | 5,000 req/h (authenticated) | Read `X-RateLimit-Remaining` header; if approaching 0, return `StorageError::WriteFailed` with retry-after |
| GitCode | 400/min, 4,000/h | Read `Retry-After` header on 429; return `StorageError::WriteFailed` |
| Both | 429 Too Many Requests | Worker retry (3 attempts, existing mechanism) handles transient rate limits |

#### Content Size Limits

| Provider | Endpoint | Limit | Fallback |
|----------|----------|-------|----------|
| GitHub | Contents API | 100 MB | N/A — PicHost max upload is 50 MB |
| GitCode | Contents API | 20 MB | `POST .../file/upload` (multipart, 20 MB) |
| GitCode | File upload | 20 MB | If >20 MB on GitCode: return `413 Payload Too Large` with message suggesting the user switch to local or GitHub storage. No silent fallback — the user made an explicit backend choice; silently changing it would be confusing. |

### 2.6 Router Changes

The `StorageRouter` needs to support per-upload backend selection via `storage_config_id` instead of the current per-user `storage_backend` column.

**New method**:
```rust
impl StorageRouter {
    /// Resolve a backend by config ID, not by name string.
    pub fn for_config(
        &self,
        config: &UserStorageConfig,
    ) -> Result<Arc<dyn StorageBackend>, StorageError> {
        match config.provider.as_str() {
            "local" => Ok(self.default_backend()),
            "github" | "gitcode" => {
                // Dynamic GitStorage creation if not pre-registered
                // (or pre-register all configured backends at startup)
                self.backends.get(&config.id.to_string())
                    .cloned()
                    .ok_or(StorageError::Config("backend not found".into()))
            }
            _ => Err(StorageError::Config(format!("unknown provider: {}", config.provider)))
        }
    }
}
```

**Registration strategy**: Git backends are NOT pre-registered in `init_storage_backends()` at server startup. They are created dynamically when a user uploads to a Git backend, and cached by `config.id` in the Router's HashMap. Rationale: a user's Git PAT could change at any time; pre-registration at startup would use stale tokens.

**Alternative**: Pre-register a "template" `GitStorage` that takes token at method-call time. This is simpler but requires changing the `StorageBackend` trait. **Decision**: dynamic creation + caching. The `StorageRouter` gains a `get_or_create_git(config)` method.

### 2.7 Upload Pipeline Changes

**File**: `pichost-api/src/services/upload.rs`

Changes to `process_upload()`:

1. Accept optional `storage_config_ids: Option<Vec<Uuid>>` parameter
2. If not provided → use user's default config (from `user_storage_configs`), fallback to `local`
3. If provided → validate each ID belongs to user, validate max 2, validate at least one is `local`
4. Loop: for each `storage_config_id`, acquire backend via `router.get_or_create_git(config)`, call `put()`, generate URL
5. Insert one `images` row **per backend** (each gets a unique `id`, `public_key`, `url` — but same `sha256`, `original_name`)
6. Enqueue one worker task per image row

**Multi-backend insert** means uploading to GitHub + local produces **2 image records**, not 1. The frontend shows 2 UploadCards (one per backend) with the same filename but different URLs.

**Dedup behavior**: The existing SHA256 dedup is per-user. With multi-backend uploads, dedup is extended to `(user_id, sha256, storage_config_id)`. This means:
- Uploading the same image to GitHub twice → dedup (2nd returns the existing GitHub row)
- Uploading the same image to GitHub first, then to local → NOT a dedup (different `storage_config_id` → new row created)
- The dedup query changes from `WHERE user_id=$1 AND sha256=$2` to `WHERE user_id=$1 AND sha256=$2 AND storage_config_id=$3`

### 2.8 API Endpoints

#### Storage Config CRUD

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET` | `/api/v1/users/me/storage-configs` | JWT | List all configs (token masked) |
| `POST` | `/api/v1/users/me/storage-configs` | JWT | Create config. Validates token + repo reachability before saving. Max 5 per user. |
| `GET` | `/api/v1/users/me/storage-configs/:id` | JWT | Single config detail (token masked) |
| `PATCH` | `/api/v1/users/me/storage-configs/:id` | JWT | Update name, token, repo, branch, path_prefix. Re-validates if token/repo changed. |
| `DELETE` | `/api/v1/users/me/storage-configs/:id` | JWT | Delete config. Returns 409 if images reference it. |
| `POST` | `/api/v1/users/me/storage-configs/:id/default` | JWT | Set as default (unsets previous default). |

#### Upload Changes

**`POST /api/v1/images`** — new optional FormData field:

```
storage_config_ids: "uuid1,uuid2"  (optional, comma-separated, max 2)
```

- Omitted → use default config (local fallback)
- 1 ID → write to that backend only
- 2 IDs → write to both (one must be `local`)

Response: `200` with `Vec<UploadResult>` (one per backend). Each `UploadResult` includes the new `storage_config` field:

```json
{
    "id": "uuid...",
    "public_key": "a3f8c2",
    "url": "https://raw.githubusercontent.com/owner/repo/main/pichost/2026/07/19/a3f8c2.png",
    "original_name": "photo.jpg",
    "storage_config": {
        "id": "uuid...",
        "name": "我的GitHub图床",
        "provider": "github"
    },
    ...
}
```

#### Gallery Filtering

**`GET /api/v1/images`** — new optional query parameter:

```
?storage_config_id=uuid
```

Adds `AND i.storage_config_id = $N` to the `WHERE` clause in `fetch_user_images()` and `count_user_images()`.

#### Image Detail

**`GET /api/v1/images/:id`** — response includes `storage_config` object (same structure as UploadResult).

### 2.9 Frontend

#### Settings Page — Storage Config Management

New section card replacing the current single `<select>` for `storage_backend`:

- List all user's configs as radio-style cards
- Each card shows: name, provider icon, repo path, default badge
- `[+ 添加存储后端]` button opens modal
- Add/Edit modal: name input, provider dropdown (GitHub/GitCode), token input (with show/hide toggle), repo input (`owner/repo`), branch input (default `main`), path prefix input (optional), "Set as default" checkbox, **"Test Connection" button** (required before save)
- Delete button with confirmation (warns if images reference this config)
- Default indicator toggle per config

#### Dashboard — Multi-Backend Upload Selector

Above DropZone:

```
存储到: [我的GitHub图床 ▾]  [+ 添加第2个后端]
         [本地存储 ▾]  (已选 2/2)
```

- Default: user's default config pre-selected
- `[+ 添加第2个后端]` expands a second dropdown
- Selected backends gray out in the other dropdown
- Max 2 total
- DropZone, clipboard paste, and URL upload all use this selector

#### UploadCard Changes

Each card shows which backend(s) the file was written to:

```
✓ photo.jpg (2.3 MB)
  → 我的GitHub图床
  [打开] [复制URL] [复制MD]
```

For dual-backend uploads, the frontend receives 2 `UploadResult` objects and renders 2 cards.

#### Gallery Filtering

Filter bar gains a `storage_config_id` dropdown:

```
[全部后端 ▾]  [🔍 搜索...]  [排序 ▾]  [全选]
```

Dropdown lists: "全部" + each user's storage config. Selecting one adds `storage_config_id` to the API request and URL search params.

#### Gallery Image Cards

Each image card shows a small provider badge (GitHub/GitCode/local icon) in the corner.

### 2.10 Worker Changes

`TaskPayload` gains:
```rust
pub storage_config_id: Option<Uuid>,
pub storage_backend_name: String,    // "github" | "gitcode" | "local"
```

Worker resolves backend via `router.for_config(config)` at task processing time. All variant writes (thumbnail, WebP) go to the same backend as the source image.

**No pipeline logic changes** — the `StorageBackend` trait abstraction handles the difference.

### 2.11 Security Constraints

| Rule | Implementation |
|------|---------------|
| Token encrypted at rest | AES-256-GCM with independent key (`PICHOST_AUTH_TOKEN_ENCRYPTION_KEY`) |
| Token never returned in API responses | GET/PATCH return `token_masked: "ghp_****abcd"` |
| Repository reachability verified on create | `POST` handler calls `GET /repos/{owner}/{repo}` before INSERT |
| Delete protection | 409 Conflict if any `images` rows reference the config |
| Config limit per user | Max 5 (configurable via env var) |
| At least one local backend in multi-upload | Enforced server-side; 400 if 2 non-local backends selected |
| Token never logged | Middleware strips `token` field from request body logging |

---

## 3. P4-B: Clipboard Paste + URL Upload

### 3.1 Scope

- Paste images from clipboard directly into the upload page
- Provide an image URL and have the server download + upload it
- Both use the same backend selection logic from P4-A

### 3.2 Clipboard Paste

**Trigger**: `paste` event on the DropZone container (or `window`).

**Detection**: Check `event.clipboardData.items` for items where `type.startsWith('image/')`.

**Flow**:
```
User presses Ctrl+V
  → paste event fires
  → clipboardData.items[n].type === 'image/png'
  → item.getAsFile() → File object
  → addFiles([file], { source: 'paste', storageConfigIds })
  → useUploadQueue processes normally
```

**Implementation**: `useClipboardPaste` hook or inline handler in Dashboard. Must handle: no image in clipboard (ignore), multiple clipboard items (take first image only).

### 3.3 URL Upload

Two implementation options:

| Option | Description | Pros | Cons |
|--------|-------------|------|------|
| **Client-side fetch** | Frontend fetches URL → Blob → File → standard upload | No new API, uses existing upload | CORS issues, double bandwidth |
| **Server-side download** | New `POST /images/upload-url` → server fetches URL → processes | No CORS, single bandwidth | New endpoint, server SSRF risk |

**Chosen**: Server-side download — avoids CORS, more reliable. Adds SSRF protection.

**New endpoint**:

```
POST /api/v1/images/upload-url
Body: { "url": "https://example.com/photo.jpg", "storage_config_ids": ["uuid1"] }
Response: 200 { ... UploadResult }
```

**SSRF protection**:
- DNS resolution check: reject private/reserved IPs (127.0.0.0/8, 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16, 169.254.0.0/16, ::1, fc00::/7)
- URL scheme allowlist: `http` and `https` only
- Timeout: 30s
- Max response size: 50 MB
- Redirect limit: 5 hops
- Content-Type validation via magic bytes after download

**Frontend**: URL input + "Upload" button next to DropZone. On submit, calls `uploadImageFromUrl()` → shows progress indicator → appends result to upload queue.

---

## 4. P4-C: Gallery Categories/Directories

### 4.1 Scope

- Users can create hierarchical categories (max 2 levels)
- Images can be assigned to categories
- Gallery can filter by category
- Bulk move images between categories

### 4.2 Database Schema

```sql
-- Migration 0009
CREATE TABLE categories (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id     UUID NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        VARCHAR(128) NOT NULL,
    parent_id   UUID REFERENCES categories(id) ON DELETE CASCADE,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    UNIQUE(user_id, name, parent_id)
);

ALTER TABLE images
    ADD COLUMN category_id UUID REFERENCES categories(id) ON DELETE SET NULL;
```

- `parent_id = NULL` means root-level category
- Max depth enforced at application level (2 levels)
- Deleting a category cascades to children; sets `images.category_id = NULL`

### 4.3 API Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `GET` | `/api/v1/categories` | JWT | List user's categories. Returns tree structure: `[{ id, name, parent_id, children: [...] }]` |
| `POST` | `/api/v1/categories` | JWT | Create category. Body: `{ name, parent_id? }`. Validates max depth ≤ 2. |
| `PATCH` | `/api/v1/categories/:id` | JWT | Rename category |
| `DELETE` | `/api/v1/categories/:id` | JWT | Delete category (cascades children, unlinks images) |
| `POST` | `/api/v1/images/:id/move` | JWT | Move image to category. Body: `{ category_id }`. |
| `POST` | `/api/v1/images/batch-move` | JWT | Bulk move. Body: `{ image_ids: [...], category_id }`. |

Gallery filtering:

```
GET /api/v1/images?category_id=uuid
```

Adds `AND (i.category_id = $N OR ($N IS NULL AND i.category_id IS NULL))` for "uncategorized" support.

### 4.4 Frontend

**Gallery layout change**: Two-panel layout.

```
┌─ 分类 ────┬── 图片网格 ──────────────────────────┐
│            │                                       │
│ 📁 全部    │  [🔍] [存储 ▾] [排序 ▾] [全选]       │
│ 📁 博客    │                                       │
│   📁 Rust  │  ┌──┐ ┌──┐ ┌──┐ ┌──┐               │
│   📁 前端  │  │  │ │  │ │  │ │  │               │
│ 📁 项目    │  └──┘ └──┘ └──┘ └──┘               │
│            │                                       │
│ [+ 新建]   │                                       │
└────────────┴───────────────────────────────────────┘
```

- Left panel: collapsible tree, max 2 levels
- Click category → filter gallery, update URL `?category=uuid`
- Right-click category → Rename / Delete / New sub-category
- Drag image to category in sidebar → move
- Batch select → "移动到分类" dropdown button → category picker modal

---

## 5. P4-D: Watermarking (Server-Side)

### 5.1 Architecture Decision: Server-Side vs Client-Side

User's original suggestion: put preprocessing on frontend. For watermarking specifically:

| | Server-Side (Worker) | Client-Side (Canvas) |
|---|---|---|
| Reliability | Always applied | Can be bypassed (curl upload, API direct) |
| Processing cost | On worker | On user's browser |
| Image quality | High (image crate, accurate rendering) | Variable (Canvas limitations) |
| Font support | `imageproc` + `rusttype` (TTF fonts) | Browser fonts only |
| Consistency | Same result for all users | Browser-dependent |

**Chosen**: Server-side in the worker pipeline. Watermarking is a security/attribution feature — it must not be bypassable. Compromise: watermark added in worker, other preprocessing (compress, resize, EXIF strip) stays on frontend.

### 5.2 Database

```sql
-- Migration 0010
ALTER TABLE users
    ADD COLUMN watermark_config JSONB;
```

Default `NULL` (watermarking disabled).

### 5.3 Watermark Config Schema

```json
{
    "enabled": true,
    "text": "@username",
    "font": "NotoSansSC-Regular",
    "font_size": 48,
    "color": "rgba(255, 255, 255, 0.5)",
    "rotation": -30.0,
    "scale": 0.15,
    "position": "bottom-right",
    "margin_x": 20,
    "margin_y": 20
}
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `enabled` | bool | `false` | Master toggle |
| `text` | string | `""` | Watermark text |
| `font` | string | `"NotoSansSC-Regular"` | Font name (from bundled fonts) |
| `font_size` | u32 | `48` | Base font size (scaled by `scale`) |
| `color` | string | `"rgba(255,255,255,0.5)"` | Text color with alpha |
| `rotation` | f64 | `-30.0` | Degrees |
| `scale` | f64 | `0.15` | Relative to image diagonal |
| `position` | enum | `"bottom-right"` | `top-left|top-right|bottom-left|bottom-right|center|tile` |
| `margin_x` | u32 | `20` | Pixels from edge |
| `margin_y` | u32 | `20` | Pixels from edge |

### 5.4 Implementation

**Crate**: `imageproc` (already in workspace deps or add) + `rusttype` for TTF rendering.

**Bundled fonts**: 5 fonts shipped with the binary (embedded via `include_bytes!`):
- `NotoSansSC-Regular.ttf` (中文)
- `NotoSans-Regular.ttf` (Latin)
- `Arial.ttf`
- `DejaVuSans.ttf`
- `FiraCode-Regular.ttf`

**Hook point**: `pichost-worker/src/pipeline.rs` → `process_image_variants()`. Insert watermark step **after** source image decode, **before** thumbnail/WebP generation. This ensures all variants carry the watermark.

```rust
// New function in processor.rs
pub fn apply_watermark(
    img: &DynamicImage,
    config: &WatermarkConfig,
) -> DynamicImage {
    if !config.enabled || config.text.is_empty() {
        return img.clone();
    }
    let font = load_font(&config.font);
    let font_size = calculate_font_size(img, config);
    let (x, y) = calculate_position(img, config, font_size);
    // ... imageproc::drawing::draw_text_mut(...)
}
```

### 5.5 API

Watermark config is part of the user profile. Extended `PATCH /api/v1/users/me`:

```json
{
    "watermark_config": {
        "enabled": true,
        "text": "@myusername",
        ...
    }
}
```

### 5.6 Frontend

Settings page — new "默认水印" card:

- Enable/disable toggle
- Text input (preview in real-time on a sample image)
- Font dropdown (5 options with preview)
- Font size slider
- Color picker (hex + alpha)
- Rotation slider (-180 to 180)
- Scale slider (0.01 to 1.0)
- Position: 3×3 grid selector + "tile" option
- Margin inputs
- **Live preview**: a sample image updates as settings change

---

## 6. P4-E: Image Preprocessing (Client-Side)

### 6.1 Scope

All preprocessing happens in the browser **before** upload. None of these operations are mandatory — users toggle them on/off.

| Operation | Implementation | Description |
|-----------|---------------|-------------|
| EXIF removal | `exif-js` | Strip all EXIF/metadata from JPEG |
| Rotation | Canvas `rotate()` + `drawImage` | Rotate by 90°/180°/270° or custom degrees |
| Resize | Canvas `drawImage` with target dimensions | Resize to max width/height, maintain aspect ratio |
| Format conversion | Canvas `toBlob(mimeType, quality)` | Convert PNG→JPEG, JPEG→WebP, etc. |
| Compression | Canvas `toBlob(type, quality)` | JPEG quality slider (10-100) |

### 6.2 Architecture

**Web Worker** — all processing happens off the main thread to avoid UI freezing.

```
Main Thread                    Web Worker
    │                              │
    ├─ File + prefs ───────────────┤
    │                              ├─ ExifReader → strip EXIF
    │                              ├─ createImageBitmap(file)
    │                              ├─ OffscreenCanvas operations
    │                              ├─ canvas.convertToBlob({type, quality})
    │                              └─ return { blob, metadata }
    │  ◄── blob + stats ─────────┤
    ├─ uploadFile(blob, name)
    │
```

**Prefs Model** (in `useUploadQueue` or a settings context):

```typescript
interface PreprocessingPrefs {
  stripExif: boolean;         // default false
  resize: {
    enabled: boolean;         // default false
    maxWidth: number;         // default 1920
    maxHeight: number;        // default 1920
  };
  formatConvert: {
    enabled: boolean;         // default false
    targetFormat: string;     // "image/jpeg" | "image/png" | "image/webp"
    quality: number;          // 0-100, default 85
  };
  compression: {
    enabled: boolean;         // default false
    quality: number;          // 0-100, default 80
    maxSizeKB: number;        // optional, default 0 (no limit)
  };
  rotate: {
    enabled: boolean;         // default false
    degrees: number;          // 0, 90, 180, 270
  };
}
```

### 6.3 Frontend Integration

**Settings page** — new "上传预处理" card:
- Toggle per operation with brief description
- Resize: width + height inputs
- Format: dropdown + quality slider
- Compression: quality slider
- Rotation: radio buttons (0°/90°/180°/270°)

**Dashboard** — preprocessing prefs shown as compact chips below DropZone:
```
预处理: [EXIF:开] [缩放:1920×1920] [WebP:85] [压缩:80%]
        [配置...]
```
Click `[配置...]` → jump to Settings.

**Processing indicator**: During preprocessing, UploadCard shows "处理中..." status before "上传中...".

### 6.4 Limitations

- Canvas-based operations are lossy for format conversion
- Not all browsers support `OffscreenCanvas` in Workers (fallback: main-thread Canvas with chunked processing)
- Large images (>20MP) may cause browser OOM — warn user
- AVIF encoding requires Chrome 85+ (fallback to WebP/JPEG)

---

## 7. P4-F: Filename Preservation + Rename

### 7.1 Scope

- `original_name` is already stored in DB (existing behavior)
- Display original name in Gallery and ImageDetail
- Allow users to rename images
- Git storage uses `original_name` as the filename (not random hex) for human-readable URLs

### 7.2 API

**`PATCH /api/v1/images/:id`** — already exists for future use, add:

```json
{
    "original_name": "new-filename.jpg"
}
```

- Validate: max 255 chars, no path separators (`/`, `\`), no null bytes
- Returns updated `UploadResult`

**URL implications**: Changing the original name after upload does **not** change the Git repository path or public URL. The Git path is fixed at upload time based on `public_key` + extension. Renaming only affects the `original_name` display field.

### 7.3 Frontend

**ImageDetail page**:
```
┌─ 图片详情 ──────────────────────────────────┐
│                                              │
│  photo.jpg  [✎]                             │
│  ↑ click → becomes input, Enter to save     │
│                                              │
│  ┌──────────────────────────────────┐       │
│  │         Image preview             │       │
│  └──────────────────────────────────┘       │
│  ...                                         │
└──────────────────────────────────────────────┘
```

**Gallery cards**: Display `original_name` in the overlay (already partially done, verify).

---

## 8. Migration Plan

### 8.1 Dependencies

```
P3 (gap fixes)
  │
  └── P4-A (Git storage + multi-backend)
        │
        ├── P4-B (clipboard + URL upload)
        ├── P4-C (categories)
        ├── P4-D (watermarking)
        ├── P4-E (preprocessing)
        └── P4-F (rename)
```

P4-B through P4-F are fully independent of each other. They can be developed in any order or in parallel.

### 8.2 Migration Files

| Migration | Creates | Phase |
|-----------|---------|-------|
| `0008` | `user_storage_configs` table, `storage_config_id` column on `images` | P4-A |
| `0009` | `categories` table, `category_id` column on `images` | P4-C |
| `0010` | `watermark_config` column on `users` | P4-D |

P4-B, P4-E, P4-F require no database migrations.

### 8.3 Version Bumps

| Phase | Version | Type |
|-------|---------|------|
| P4-A | v0.15.0 | Minor (significant new feature) |
| P4-B | v0.15.1 | Patch |
| P4-C | v0.16.0 | Minor |
| P4-D | v0.16.1 | Patch |
| P4-E | v0.16.2 | Patch |
| P4-F | v0.16.3 | Patch |

### 8.4 Config Changes

| Variable | Phase | Required |
|----------|-------|----------|
| `PICHOST_AUTH_TOKEN_ENCRYPTION_KEY` | P4-A | Yes (for Git backends) |
| `PICHOST_STORAGE_GITHUB_ENABLED` | P4-A | No (default: true if encryption key set) |
| `PICHOST_STORAGE_GITCODE_ENABLED` | P4-A | No (default: true if encryption key set) |
| `PICHOST_STORAGE_MAX_USER_CONFIGS` | P4-A | No (default: 5) |

---

## 9. Risk Assessment

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| GitCode API instability / breaking changes | Medium | High | Abstract via `GitProvider` enum; can disable GitCode via config flag without affecting GitHub |
| User PAT leakage (log, error message, response) | Medium | Critical | AES-256-GCM at rest; token masking in all responses; logging middleware strips `token` field; code review checklist |
| GitCode 20MB limit blocks large uploads | Medium | Medium | Auto-fallback to local storage for files >20MB on GitCode; configurable threshold |
| GitHub/GitCode rate limit causes upload failures | Medium | Low | Worker retry (3×) covers transient limits; client shows clear "rate limited, retry in N seconds" error |
| SSRF via URL upload | Medium | High | IP blocklist (all private ranges), scheme allowlist (http/https), magic byte validation post-download |
| AES key rotation breaks existing tokens | Low | High | Support key versioning: store `token_encrypted: "v1:base64ciphertext"`, try all known keys on decrypt |
| Category depth unlimited → complex UI | Low | Low | Enforce max 2 levels at API level; UI only supports 2 |
| Canvas preprocessing inconsistent across browsers | Medium | Low | Document browser requirements; fallback to server-side processing for unsupported browsers |

---

## Appendix A: GitCode vs GitHub API Compatibility

| Dimension | GitHub | GitCode | Compatible? |
|-----------|--------|---------|-------------|
| Base URL | `https://api.github.com` | `https://api.gitcode.com/api/v5` | Different (config) |
| Auth header | `Authorization: Bearer <token>` | `Authorization: Bearer` or `PRIVATE-TOKEN` | ✅ Both support Bearer |
| Create file | `PUT .../contents/{path}` | `POST .../contents/{path}` | Different HTTP method |
| Get file | `GET .../contents/{path}` | `GET .../contents/{path}` | ✅ Same |
| Get raw file | `GET .../raw/{path}` | `GET .../raw/{path}` | ✅ Same |
| Delete file | `DELETE .../contents/{path}` | `DELETE .../contents/{path}` | ✅ Same |
| File size limit | 100 MB (Contents API) | 20 MB (Contents API) | Different |
| Rate limit | 5,000 req/h | 400/min, 4,000/h | Similar |
| Raw URL pattern | `raw.githubusercontent.com/{owner}/{repo}/{branch}/{path}` | `raw.gitcode.com/{owner}/{repo}/{branch}/{path}` | Same pattern |

**Conclusion**: Single `GitStorage` implementation with `GitProvider` enum switching base URL, HTTP method, and raw URL template. Core parameter structures are identical.

## Appendix B: GDPR/Privacy Note

Git backends store images in user-owned repositories. PicHost itself does not have access to the repository content outside of the PAT the user provides. Users can revoke PATs at any time on GitHub/GitCode to cut off PicHost's access. Image data location is under the user's control — this is a privacy-positive design pattern.
