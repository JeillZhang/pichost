# PicHost — Agent Guide

## Workspace

- Cargo workspace: `pichost-core`, `pichost-api`, `pichost-worker`.
- Rust edition 2021, stable toolchain with `rustfmt` + `clippy` (see `rust-toolchain.toml`). No custom fmt/clippy config.
- Frontend: `web-ui/` — independent npm project (React 19, Vite 8, Tailwind CSS 4, TypeScript 7).
- Version: `0.16.1` — P4-D complete. Server-side watermark overlay with configurable text/position/font. Bump patch for fixes, minor for features.

## Key Commands

| Action | Command | Notes |
|---|---|---|
| Build all | `cargo build --workspace` | |
| Check only api | `cargo check -p pichost-api` | Fast compile-check |
| Test all | `cargo test --workspace` | 63 pass, 10 ignored (need DB/Redis/S3) |
| Lint | `cargo clippy --workspace -- -D warnings` | Zero warnings required |
| Run API server | `cargo run -p pichost-api` | Requires PostgreSQL + Redis |
| Frontend dev | `cd web-ui && npm run dev` | Vite proxies `/api`, `/u` → `localhost:3000` |
| Frontend build | `cd web-ui && npm run build` | `tsc -b && vite build` |
| Docker stack | `docker compose up --build -d` | Nginx :80, API×2, Worker×2, PG, Redis |
| Docker stop | `docker compose down` | Add `-v` to wipe volumes |

## Setup Gotchas

- **Copy `.env.example` → `.env`, edit `PICHOST_AUTH_JWT_SECRET`** (min 32 chars).
- **Two DB URL vars**: `DATABASE_URL` (sqlx CLI helper, not consumed by app) and `PICHOST_DATABASE_URL` (consumed by figment config). For local dev only `PICHOST_DATABASE_URL` matters.
- **sqlx queries are runtime-only** (uses `query_as`, `query_scalar` — no `query!` macro). No compile-time DB needed, no `sqlx prepare`.
- **Migrations auto-apply** at API startup via `sqlx::migrate!()`. 10 migrations: `0001`-`0010`.
- `storage-local/` is gitignored, created at runtime by LocalStorage.
- Prerequisites: Rust 1.96+, Node.js 22+, PostgreSQL 18, Redis 8.

## Config System

- Uses `figment` crate: defaults → `config.toml` (optional) → `PICHOST_*` env vars.
- Config struct in `pichost-core/src/config.rs` — has `Default` impl with dev defaults.
- All env vars use `PICHOST_` prefix. Key vars:
  - `PICHOST_DATABASE_URL`, `PICHOST_REDIS_URL` — runtime connections
  - `PICHOST_AUTH_JWT_SECRET` — JWT signing key
  - `PICHOST_SERVER_PUBLIC_URL` — for OAuth callbacks and link generation
  - OAuth: `PICHOST_AUTH_OAUTH_GITHUB_CLIENT_ID`, `..._SECRET`, same for Google
  - `PICHOST_STORAGE_LOCAL_BASE_PATH`, `PICHOST_STORAGE_RUSTFS_*` — storage config
  - `PICHOST_STORAGE_MAX_USER_CONFIGS` — max Git storage configs per user (default 5)
  - `PICHOST_AUTH_TOKEN_ENCRYPTION_KEY` — AES-256-GCM key for Git token encryption
- No `config.toml` in repo — env vars are the intended override mechanism.

## CRATE BOUNDARIES

- **pichost-core** (`pichost_core`): Domain models, config, error types, `StorageBackend` trait + `LocalStorage`/`RustfsStorage`/`GitStorage` impls + `StorageRouter`. No web/framework deps.
- **pichost-api** (`pichost_api`): Axum server — routes, middleware, services, DB pool, Redis cache. Depends on `pichost-core`.
- **pichost-worker**: Background image processing binary — thumbnail/WebP generation via Redis queue. Depends on `pichost-core`.

## Architecture Notes

### Auth
- JWT HS256 via `jsonwebtoken`. Access TTL = 900s, refresh TTL = 30 days.
- Redis blacklist: `bl:{jti}` for logout. Blacklist check **fails closed** (`unwrap_or(true)`) — Redis down = all auth fails.
- OAuth: GitHub/Google OAuth2 via `oauth2` crate. Users must register via invite code first, then link OAuth in Settings. Callback URLs: `{public_url}/api/v1/auth/oauth/{provider}/callback`.

### Gallery Categories
- Users can create a 2-level category hierarchy via `categories` table (migration 0009).
- Images assigned via `category_id` FK with `ON DELETE SET NULL`.
- Category CRUD at `/api/v1/categories`, image move at `/images/:id/move` and batch-move at `/images/batch-move`.
- Gallery supports `?category_id=` filter parameter.
- **Frontend CategoryTree**: sidebar component with self-contained inline CRUD — right-click context menu (rename/delete), inline rename input, create modal, delete confirmation dialog. Uses TanStack Query `useMutation` + `invalidateQueries`.

### Upload
- Multipart → magic byte check (`infer::is_image`) → SHA256 hash → per-user dedup → random 6-char hex public key → write storage → INSERT (status=`'active'`) → enqueue worker task.
- **URL upload**: `POST /images/upload-url` downloads image from URL with SSRF protection (scheme allowlist, DNS-level private IP blocking, redirect/size/timeout limits), then feeds into the same `process_upload()` pipeline.
- Dedup: per-user, per-SHA256. Same user, same content → 200 with existing metadata.
- Storage quota: enforced before write. `SUM(file_size)` per user, 413 on exceed. NULL = unlimited, default 1 GB.
- Multi-file: frontend `useUploadQueue` hook, MAX_CONCURRENT=3, per-file UploadCard progress.

### Storage Backends
- **LocalStorage**: filesystem-based, base path `./storage-local/` (configurable).
- **RustfsStorage**: S3-compatible object storage via `aws-sdk-s3`. Supports custom endpoint for non-AWS providers (MinIO, etc.).
- **GitStorage**: push files to GitHub/GitCode repositories via Contents REST API. No clone-commit-push — API direct write.
  - Tokens encrypted at rest via AES-256-GCM (`PICHOST_AUTH_TOKEN_ENCRYPTION_KEY`).
  - Per-user storage configs stored in `user_storage_configs` table, managed via `/api/v1/users/me/storage-configs` CRUD.
  - Rate limits: GitHub 5,000 req/h, GitCode 400 req/min. 429 → retry-after.
  - Size limits: GitCode 20 MB, GitHub 100 MB (PicHost's own 50 MB cap applies first).
- **StorageRouter**: `RwLock<HashMap>` for dynamic backend routing via `storage_config_id`. Git backends created/cached on demand, evicted on config change.

### Public serving
- `GET /u/{public_key}` → `Cache-Control: public, max-age=31536000, immutable`.
- Nginx proxy_cache on `/u/` and `/t/` (IMAGE_CACHE 50MB/1h).
- Status check: only `'active'` or `'ready'` images served — others return 404.

### Font embedding (watermark)
- `pichost-worker/src/fonts.rs`: `load_font()`, `builtin_font_names()`, `scaled_font_size()`.
- 5 built-in TTF fonts at `pichost-worker/fonts/`: NotoSansSC-Regular, NotoSans-Regular, Arial, DejaVuSans, FiraCode-Regular.
- Uses `rusttype` for font parsing, `imageproc` for image drawing, `ab_glyph` for font loading in watermark pipeline.

### Watermark
- `pichost-worker/src/watermark.rs`: `apply_watermark()` — text overlay on `DynamicImage` with position/color/tile support.
- Applied in `process_task()` between `read_source_image()` and `process_image_variants()` — all variants inherit the watermark.
- Config stored as JSONB on `users.watermark_config` (migration `0010`). `WatermarkConfig` and `WatermarkPosition` types in `pichost_core::models`.
- PATCH endpoints (`/users/me`, `/admin/users/:id`) accept `watermark_config` with absent/null/value semantics. Admin `AuthUser` middleware reads watermark_config.
- Frontend: `WatermarkSettings` component in Settings page — enable toggle, text/font/color/position/rotation/scale fields.
- Watermark is enabled only when `watermark_config.enabled == true` and `text` is non-empty. Disabled/empty → no-op (returns clone).

### Image status quirk
- DB default is `'pending'`, but upload INSERT hardcodes `'active'`. The `ImageStatus` enum has `Pending/Processing/Ready/Failed` but code checks string `"active"`. If adding status transitions, reconcile this.

### Rate limiting
- 4 strategies in Redis middleware: auth (5/min/IP), upload (30/min/user), general (60/min/user), public images (200/min/IP).
- Nginx layer: additional `limit_req` zones (60r/m API, 200r/m public).

### Deployment
- Nginx :80 → API upstream `least_conn` (2 replicas).
- Worker: 2 replicas, independent Redis `BRPOP` consumers.
- API is stateless (state in PostgreSQL + Redis) — scale horizontally.
- Postgres/Redis ports not exposed to host — internal Docker network only.

## API Endpoints Summary

| Method | Path | Auth | Notes |
|--------|------|------|-------|
| POST | `/auth/register` | No | Invite code required (unless first user → auto-admin) |
| POST | `/auth/login` | No | |
| POST | `/auth/refresh` | Refresh | |
| POST | `/auth/logout` | JWT | |
| GET | `/auth/oauth/{github,google}` | No | Redirect to provider |
| GET | `/auth/oauth/{provider}/callback` | No | Returns JWT |
| POST | `/images` | JWT | Multipart upload |
| POST | `/images/upload-url` | JWT | Upload from URL (SSRF-protected) |
| GET | `/images` | JWT | Paginated: `page`, `per_page` (default 20, max 100), `sort` (created_at/file_size/original_name), `order` (asc/desc), `search` (ILIKE) |
| GET | `/images/:id` | JWT | |
| DELETE | `/images/:id` | JWT | |
| POST | `/images/:id/move` | JWT | Move image to category: `{ category_id }` |
| POST | `/images/batch-delete` | JWT | `{ ids: UUID[] }`, max 100 |
| POST | `/images/batch-move` | JWT | Batch move to category: `{ image_ids: [...], category_id }`, max 100 |
| GET | `/u/:public_key` | No | Public image serve |
| GET | `/u/thumb/:id` | No | Thumbnail |
| GET | `/u/webp/:id` | No | WebP |
| GET | `/users/me/stats` | JWT | Includes `storage_quota` |
| GET/POST | `/categories` | JWT | Category CRUD: GET tree, POST create `{ name, parent_id? }` |
| GET/PATCH/DELETE | `/categories/:id` | JWT | Single category: GET, PATCH rename, DELETE cascades |
| GET/POST | `/users/me/storage-configs` | JWT | Git storage config CRUD. GET all, POST create |
| GET/PATCH/DELETE | `/users/me/storage-configs/:id` | JWT | Single config: GET, PATCH update, DELETE |
| POST | `/users/oauth/link` | JWT | `{ provider, code }` |
| GET | `/admin/stats` | JWT+Admin | |
| GET/POST | `/admin/invites` | JWT+Admin | |
| GET | `/admin/users` | JWT+Admin | Paginated, includes quota |
| PATCH | `/admin/users/:id` | JWT+Admin | Fields + `storage_quota` |
| DELETE | `/admin/users/:id` | JWT+Admin | Cascades |
| GET | `/metrics` | No | Prometheus text format |
| GET | `/health` | No | Nginx health check; also `/api/health` (JSON) |

## Testing

- **Unit tests** (38 pass): `storage_test.rs` (4), `gallery_test.rs` (8), `category_test.rs` (5), `admin_test.rs` (6 ignored — need DB/Redis), `pichost-api` unit (11), `pichost-core` (8), `health_test.rs` (1 ignored), `rustfs_test.rs` (2 pass + 3 ignored — need S3).
- **Run focused**: `cargo test -p pichost-api test_image_list` — matches test name prefix.
- **pichost-core tests** need `tokio` features `["rt", "macros"]`.
- Integration tests in `pichost-api/tests/` require PostgreSQL + Redis (ignored by default).
- No frontend tests.

## Frontend (web-ui/)

- React 19, Vite 8, Tailwind CSS 4, TypeScript 7.
- State: Zustand (client) + TanStack Query v5 (server).
- HTTP: `ky`. Routing: `react-router-dom` v7. Upload: `react-dropzone`. Toasts: `sonner`.
- Entry: `src/main.tsx` → `App.tsx`. Dev server :5173, proxy to :3000.
- **CSS variables**: Design system uses `var(--color-*)` tokens for theming. Glass effects via `backdrop-blur-sm`, `bg-[var(--glass-bg)]`, `border-[var(--color-border)]`.
- **Hooks**: `useUploadQueue` (multi-file upload with concurrency pool), `useInfiniteQuery` (Gallery scroll).
- **Components**: `CategoryTree` (sidebar with inline CRUD — context menu, rename, delete confirmation, create modal).

## Rules

- Commit messages in English. `docs/superpowers/specs/` docs in Chinese.
- Docs under `docs/` are tracked deliverables — commit them.
- Bump version on every feature (`0.1.0` → `0.2.0`) and bugfix (`0.1.0` → `0.1.1`).
- Before planning/developing, read `.omo/summary/summary_and_next.md` and `docs/superpowers/` first.
- All diagrams in spec docs under `docs/superpowers/specs/` must use UML or Mermaid modeling diagrams — no other diagram formats.
- Update `docs/superpowers/specs` target spec docs TODO list after each phase.
- After each plan completes, update `.omo/summary/summary_and_next.md` to document:
  - What features have been implemented in this phase
  - What features are still pending/unimplemented
  - The next plan / next steps
  - Any remaining issues or known limitations
- After each feature phase completes AND `cargo test --workspace` + `cargo clippy --workspace -- -D warnings` pass, automatically:
  - Update `AGENTS.md`: sync version, migrations count, new API routes, architecture notes, config vars, crate boundaries — any structural change introduced in the phase.
  - Update `README.md`: sync version tagline, Features checklist, Project Structure tree, API endpoint tables, migrations count, and config var table — any user-facing change introduced in the phase.
  - Update `.omo/summary/summary_and_next.md`: add a new "## {phase}: {title} ✅ (本次完成)" section documenting what was built, verification results, and updating the "## 待实施" table if needed.
  - Commit the three files together as `docs: auto-sync AGENTS.md, README.md, summary after {phase} completion`.
  - Do NOT wait for the user to request this — it is a mandatory post-phase step.
- Clean up temp files, Docker containers after each development phase.
- When a command hangs >120s, cancel and retry.
- PR creation: create the PR and share the link — the user handles merge.
- Rust function or method should less than 50 lines, and <= 120 characters for each line.
