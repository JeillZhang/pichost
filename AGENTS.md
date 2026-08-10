# PicHost — Agent Guide

## Workspace

- Cargo workspace: `pichost-core`, `pichost-api`, `pichost-worker`.
- Rust edition 2021, stable toolchain with `rustfmt` + `clippy` (see `rust-toolchain.toml`). No custom fmt/clippy config.
- Frontend: `web-ui/` — independent npm project (React 19, Vite 8, Tailwind CSS 4, TypeScript 7).
- Version: `0.21.0` — SQLite lite mode (dual DB modes: standard PostgreSQL+Redis / lightweight SQLite with embedded in-process worker, zero external dependencies) + image detail zoom viewer (fullscreen lightbox: wheel/drag/pinch zoom, toolbar + keyboard controls) + responsive layout complete (mobile-first: hamburger nav, category sheet, touch menus, bottom-sheet dialogs, cardified admin tables) + i18n complete (en/zh-CN via i18next + LanguageSwitcher, localized API errors with error codes (`{"error","code"}` envelope + Accept-Language negotiation), deployment language config (`PICHOST_I18N_LANGUAGE` + admin config hot reload).

## Key Commands

| Action | Command | Notes |
|---|---|---|
| Build all | `cargo build --workspace` | |
| Check only api | `cargo check -p pichost-api` | Fast compile-check |
| Test all | `cargo test --workspace` | 405 pass without infra; 677 pass, 0 fail with `-- --include-ignored` (Docker PG+Redis+MinIO, see Testing) |
| Lint | `cargo clippy --workspace -- -D warnings` | Zero warnings required |
| Run API server | `cargo run -p pichost-api` | Standard mode requires PostgreSQL + Redis; lite mode: `PICHOST_DATABASE_MODE=sqlite` + `PICHOST_DATABASE_URL=sqlite:///path/pichost.db` (embedded worker, no Redis) |
| Frontend dev | `cd web-ui && npm run dev` | Vite proxies `/api`, `/u` → `localhost:3000` |
| Frontend build | `cd web-ui && npm run build` | `tsc -b && vite build` |
| Verify release pkg | `bash scripts/verify-release.sh` | Local simulation of release.yml build→package→install dry-run; run before tagging `v*` |
| Docker stack | `docker compose up --build -d` | Nginx :80, API×2, Worker×2, PG, Redis |
| Docker stop | `docker compose down` | Add `-v` to wipe volumes |

## Setup Gotchas

- **Copy `.env.example` → `.env`, edit `PICHOST_AUTH_JWT_SECRET`** (min 32 chars).
- **Two DB URL vars**: `DATABASE_URL` (sqlx CLI helper, not consumed by app) and `PICHOST_DATABASE_URL` (consumed by figment config). For local dev only `PICHOST_DATABASE_URL` matters.
- **sqlx queries are runtime-only** (uses `query_as`, `query_scalar` — no `query!` macro). No compile-time DB needed, no `sqlx prepare`.
- **Migrations auto-apply** at API startup via `sqlx::migrate!()`. Two dirs: `migrations/` (PostgreSQL, 10 files `0001`-`0010`) and `migrations-sqlite/` (SQLite, 11 files `0001`-`0011` — `0011` adds lite state tables `pending_tasks`/`token_blacklist`/`rate_limits`/`invite_codes`). Driver-specific `run_pg_migrations`/`run_sqlite_migrations` pick the right dir.
- `storage-local/` is gitignored, created at runtime by LocalStorage.
- Prerequisites: Rust 1.96+, Node.js 22+. Standard mode: PostgreSQL 18 + Redis 8. Lite mode needs neither.

## Config System

- Uses `figment` crate: defaults → `config.toml` (optional) → `PICHOST_*` env vars.
- Config struct in `pichost-core/src/config.rs` — has `Default` impl with dev defaults.
- All env vars use `PICHOST_` prefix. Key vars:
  - `PICHOST_DATABASE_MODE` — `postgres` (default) | `sqlite`; sqlite selects lite mode: single-process API with embedded worker, no Redis
  - `PICHOST_DATABASE_URL`, `PICHOST_REDIS_URL` — runtime connections (lite mode URL example: `sqlite:///path/pichost.db`; `PICHOST_REDIS_URL` unused in sqlite mode)
  - `PICHOST_AUTH_JWT_SECRET` — JWT signing key
  - `PICHOST_SERVER_PUBLIC_URL` — for OAuth callbacks and link generation
  - OAuth: `PICHOST_AUTH_OAUTH_GITHUB_CLIENT_ID`, `..._SECRET`, same for Google
  - `PICHOST_STORAGE_LOCAL_BASE_PATH` — local storage dir; `PICHOST_STORAGE_RUSTFS_ENDPOINT`/`..._BUCKET`/`..._REGION`/`..._ACCESS_KEY`/`..._SECRET_KEY` — S3-compatible storage
  - `PICHOST_STORAGE_MAX_USER_CONFIGS` — max Git storage configs per user (default 5)
  - `PICHOST_TOKEN_ENCRYPTION_KEY` — AES-256-GCM key for Git token encryption (base64-encoded, 32 bytes)
  - Rate limit overrides: `PICHOST_RATE_LIMIT_AUTH_MAX`, `PICHOST_RATE_LIMIT_UPLOAD_MAX`, `PICHOST_RATE_LIMIT_GENERAL_MAX`, `PICHOST_RATE_LIMIT_PUBLIC_MAX`
  - `PICHOST_I18N_LANGUAGE` — deployment default UI language (default `"en"`, e.g. `"zh-CN"`)
  - `PICHOST_I18N_LOCALES_DIR` — optional external locale override dir (per-language subdirs, merge-override)
- **Config separator**: `.env.example` uses `__` double-underscore for nested keys (`PICHOST_AUTH__JWT_SECRET` → `auth.jwt_secret`). Single `_` also works for 2-segment flat keys (`PICHOST_DATABASE_URL` → `database.url`). Docker compose (both `docker-compose.yml` and `docker-compose.prod.yml`) uses single `_`.
- No `config.toml` in repo — env vars are the intended override mechanism.

## CRATE BOUNDARIES

- **pichost-core** (`pichost_core`): Domain models, config, error types, `StorageBackend` trait + `LocalStorage`/`RustfsStorage`/`GitStorage` impls + `StorageRouter`. Also `db` module (per-driver `create_pg_pool`/`create_sqlite_pool`, `run_pg_migrations`/`run_sqlite_migrations`, `DbType`/`DbRow`/`DbQueryResult` markers, `db_error_kind` mapping) and `state` module (5 state traits — `Queue`/`Blacklist`/`RateLimiter`/`InviteStore`/`Cache` — with SQLite impls `SqliteQueue`/`SqliteBlacklist`/`SqliteRateLimiter`/`SqliteInviteStore` + `NoopCache`). No web/framework deps.
- **pichost-api** (`pichost_api`): Axum server — routes, middleware, services, DB pool, Redis cache, system config service (config.toml read/write, backups, connection tests). Generic `AppState<DB: DbType>` + `configure_app<DB>` router assembly; `run_with::<DB>()` (standard, Redis state components) / `run_with_sqlite` (lite mode, SQLite components + embedded worker). Redis trait impls (`RedisBlacklist`/`RedisRateLimiter`/`RedisInviteStore`/`Cache`) live here. Depends on `pichost-core`. Extra deps: `toml_edit` (0.22), `regex` (1), `thiserror`, `tempfile` (dev).
- **pichost-worker**: Background image processing — thumbnail/WebP generation via Redis queue. Library + binary: `pichost_worker::process_task` is exposed so lite mode can run the worker embedded in the API process (`RedisQueue` lives here). Depends on `pichost-core`.

## Architecture Notes

### Dual DB modes (standard / lite)
- **Standard mode** (default): PostgreSQL + Redis + separate worker binary (2 replicas via Docker). State (queue/blacklist/rate limiter/invites/cache) lives in Redis, images in PG, processing in `pichost-worker`.
- **Lite mode** (`PICHOST_DATABASE_MODE=sqlite`): single-process API with an **embedded in-process worker** (`tokio::spawn(lite_worker_task)` dequeues → `pichost_worker::process_task` → ack/nack). Zero external dependencies — no Redis, no PG, no separate worker binary. State tables come from migration `0011` (`pending_tasks`/`token_blacklist`/`rate_limits`/`invite_codes`); the cache is a no-op (`NoopCache`, fail-open reads).
- **A′ generic architecture**: no `AnyPool` (sqlx Any driver lacks `Uuid`/`DateTime`/`Json` support) — instead concrete per-driver pools: `create_pg_pool`/`create_sqlite_pool` (`pichost-core/src/db.rs`), generic `AppState<DB: DbType>` + `configure_app<DB>` router assembly, and `run_with::<DB>()` (standard, Redis components) / `run_with_sqlite` (lite, SQLite components). Handlers are generic functions instantiated per driver at compile time (zero runtime dispatch). `DbType`/`DbRow`/`DbQueryResult` markers carry the generic bounds; `db_error_kind` maps driver errors.
- **5 state traits**: `Queue`/`Blacklist`/`RateLimiter`/`InviteStore`/`Cache` in `pichost-core/src/state/mod.rs` with dual implementations — Redis (`RedisQueue` in pichost-worker, `RedisBlacklist`/`RedisRateLimiter`/`RedisInviteStore`/`Cache` in pichost-api) and SQLite (`SqliteQueue`/`SqliteBlacklist`/`SqliteRateLimiter`/`SqliteInviteStore` in pichost-core) — assembled as trait objects via `build_state_components` (standard) / `build_lite_state_components` (lite).
- **Dialect-neutral SQL**: no `ILIKE` (uses `LOWER(col) LIKE LOWER($n)`), no `ANY($1)` (dynamic `IN`), no `now()` (Rust-side `DateTime<Utc>` params / `CURRENT_TIMESTAMP`).

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
  - Tokens encrypted at rest via AES-256-GCM (`PICHOST_TOKEN_ENCRYPTION_KEY`).
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
- 4 strategies in middleware (auth: 5/min/IP, upload: 30/min/user, general: 60/min/user, public images: 200/min/IP) — backed by Redis (`RedisRateLimiter`) in standard mode, SQLite `rate_limits` table (`SqliteRateLimiter`) in lite mode.
- Nginx layer: additional `limit_req` zones (60r/m API, 200r/m public).

### Settings UI
- NavBar uses a `DropdownMenu` (`web-ui/src/components/ui/DropdownMenu.tsx`) for the user section — Settings/Admin/Logout.
- Settings page is restructured as accordion sections (Profile, Password, Storage Usage, Storage Backends, Watermark, Preprocessing, OAuth) with hash-based auto-expand (`#settings?section=...`).
- Design token `--color-surface-hover` added to theme.css.

### System Config
- Config service in `pichost-api/src/services/config.rs` — reads/writes `config.toml` with figment-compatible nested keys (`[database] url`, `[redis] url`, `[server] public_url`, `[storage] default_backend`/`local_base_path`, `[i18n] language`/`locales_dir`), timestamped `.bak` backups, `test_database_connection` (5s timeout) / `test_redis_connection`.
- Admin config API (6 JWT+Admin endpoints under `/admin/config`): GET current config (sensitive fields masked), PUT write with auto-backup, POST test connections, POST backup, GET backups list, POST restore.
- Frontend: `SystemConfig.tsx` component (Database/Redis/Server/Security/Localization/Backups sections, test-connection buttons, save/restore) wired to a "Config" tab in the Admin page.
- Design tokens `--color-success` / `--color-success-hover` / `--color-success-subtle` added to theme.css.

### 国际化 (i18n)
- **Backend module** (`pichost-core/src/i18n.rs`): `Language` enum (En/ZhCN), `I18n` struct with `t(locale, key)` / `t_args(locale, key, args)` fallback chain (locale → en → key). Global singleton `RwLock<Option<Arc<I18n>>>` with lazy 5s mtime hot-check via `I18n::global()`, plus `init_global`/`reload_global`/`maybe_reload`.
- **Message catalogs**: `pichost-core/src/i18n/locales/{en,zh-CN}/messages.toml` (110 keys), embedded via `include_str!`. Optional external override dir via `PICHOST_I18N_LOCALES_DIR` (per-language subdirs, merge-override).
- **Error envelope (breaking)**: all API errors now `{"error": <localized message>, "code": <key>}`. Route-level keys are dotted (`auth.invalid_credentials`, `image.not_found`, `validation.body_invalid`); `AppError::code()` emits coarse underscore codes (`auth_failed`, `validation_error`, etc.) for internal paths — dual convention is intentional.
- **Accept-Language negotiation**: per request header → deployment `i18n.language` → en. Frontend sends `Accept-Language: <UI language>` on every request (ky beforeRequest hook).
- **JsonBody<T> extractor** (`pichost-api/src/i18n_ext.rs`) replaces `Json<T>`: malformed JSON / wrong content-type → 422/415 with `{"error": localized, "code": "validation.body_invalid"}` instead of axum plain-text. Also `Locale` extractor (FromRequestParts for AppState + Arc<AppState>) + `error_json`/`error_json_args`/`error_json_extra`.
- **Hot reload**: config service reads/writes `[i18n] language` / `[i18n] locales_dir`; PUT `/admin/config` and POST `/admin/config/restore` trigger `I18n::reload_global` — language changes apply WITHOUT restart. Admin SystemConfig UI has a Localization section (language selector).
- **Frontend stack**: i18next ^26 + react-i18next ^17 + i18next-browser-languagedetector ^8. `web-ui/src/i18n/` — init, `getCurrentLocale`, `applyLang` (syncs `<html lang>`), catalogs `{en,zh-CN}.json` (364 keys, key-set equality tested), `types/i18next.d.ts` (typed t() keys), index.html FOUC inline script, main.tsx wraps app in `I18nextProvider`.
- **LanguageSwitcher** component: NavBar + Login/Register pages; localStorage key `pichost-locale`.
- **Format module**: `web-ui/src/lib/format.ts` + `web-ui/src/hooks/useFormat.ts` — shared locale-aware formatBytes/formatDate/formatNumber (replaced 5 duplicated implementations + hardcoded 'en-US' dates).
- **API error parsing**: `web-ui/src/api/errors.ts` (getErrorCode/isErrorCode); `client.ts` beforeError hook sets `error.message` from backend localized body + attaches code.

### Deployment
- Nginx :80 → API upstream `least_conn` (2 replicas).
- Worker: 2 replicas, independent Redis `BRPOP` consumers.
- API is stateless (state in PostgreSQL + Redis) — scale horizontally.
- Postgres/Redis ports not exposed to host — internal Docker network only.
- Two compose files: `docker-compose.yml` (local dev/S3) and `docker-compose.prod.yml` (production S3, `.env`-driven).
- Bare-metal packaging: `scripts/pichost-api.service` + `scripts/pichost-worker.service` (systemd, `User=pichost`, `EnvironmentFile=/etc/pichost/.env`), install/uninstall via `scripts/install.sh` / `scripts/uninstall.sh`. `install.sh` is interactive (`--yes` unattended / `--mode postgres|sqlite`): sqlite mode skips the postgresql/redis dependency checks, installs no `pichost-worker.service`, and writes `sqlite://` into `.env` — zero external deps. Pre-tag verification: `scripts/verify-release.sh` (local mirror of release.yml build→package→install dry-run; also smoke-tests sqlite mode).
- CI: `.github/workflows/smoke-test.yml` — PR to `main` → full API integration suite (`cargo test --workspace -- --include-ignored`, ~675 tests) against PG+Redis+MinIO service containers + clippy gate. `.github/workflows/release.yml` — `v*` tags → build x86_64-unknown-linux-gnu, test + clippy, package `.tar.gz`; `-rc/-beta/-alpha/-pre` tag suffixes or manual dispatch mark the release as draft/pre-release. `.github/workflows/e2e.yml` — Playwright E2E (73 specs), PG+Redis service containers. Release body links `CHANGELOG.md` (Keep a Changelog format, updated per release).

## API Endpoints Summary

All paths below are relative to `/api/v1/` prefix unless otherwise noted. The `/u/`, `/metrics`, and `/health` paths are at root level (no `/api/v1/` prefix). All API errors return `{"error": <localized message>, "code": <error key>}` (see the 国际化 (i18n) section).

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
| PATCH | `/images/:id` | JWT | Rename: `{ original_name }` |
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
| GET | `/admin/config` | JWT+Admin | Current config, sensitive fields masked |
| PUT | `/admin/config` | JWT+Admin | Write config.toml (auto-backup), returns updated config |
| POST | `/admin/config/test` | JWT+Admin | Test DB/Redis connections |
| POST | `/admin/config/backup` | JWT+Admin | Create timestamped backup |
| GET | `/admin/config/backups` | JWT+Admin | List backup files, newest first |
| POST | `/admin/config/restore` | JWT+Admin | Restore config.toml from a backup |
| GET | `/metrics` | No | Prometheus text format |
| GET | `/health` | No | Nginx health check; also `/api/health` (JSON) |

## Testing

- **Full suite**: `cargo test --workspace` → **405 pass, 0 fail** without infra (270 DB/Redis/S3 tests `#[ignore]`-gated). With Docker PG+Redis+MinIO running: `cargo test --workspace -- --include-ignored` → **677 pass, 0 fail**. Env-sensitive config tests are hermetic via `PichostEnvGuard` (snapshot/restore of `PICHOST_*` vars), and the rustfs storage tests run against live MinIO with `PICHOST_STORAGE_RUSTFS_*` set.
- **CI**: every PR to `main` runs the full suite automatically via `.github/workflows/smoke-test.yml` (see the smoke test design guide `docs/superpowers/specs/2026-08-02-pichost-smoke-test-design.md`). New API features must add a smoke test before coding (TDD).
- **Coverage**: `cargo llvm-cov --workspace --ignore-filename-regex 'tests/|test_' -- --include-ignored` → **91.56% line coverage**. `cargo-llvm-cov` must be installed (`cargo install cargo-llvm-cov`).
- **Test infrastructure**: `pichost-api/tests/common/mod.rs` harness builds a real `AppState` (PG+Redis) + production router (`configure_app`) and drives it via `tower::ServiceExt::oneshot`. The router-assembly functions live in `pichost-api/src/app.rs` (moved from `main.rs`) so integration tests exercise the exact production routing.
- **Docker test services**: `postgres:18-alpine` (:5432), `redis:8-alpine` (:6379), `minio/minio` (:9000, bucket `pichost`, minioadmin/minioadmin). Used for the previously-ignored DB/Redis/S3 integration tests.
- **Test conventions**: async tests use `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]` (current-thread runtime deadlocks deadpool-redis/sqlx). Each test creates its own small PG pool (≤5 conns). Env-sensitive tests isolate `PICHOST_*` vars via a `PichostEnvGuard` snapshot/restore helper. Config-endpoint tests use `serial_test::serial` and clean up `config.toml`.
- **Run focused**: `cargo test -p pichost-api test_image_list` — matches test name prefix. Frontend: `cd web-ui && npx vitest run` (42 tests) + Playwright E2E (73 specs).
- Integration test files in `pichost-api/tests/`: `auth_test`, `images_test`, `users_test`, `categories_test`, `admin_test`, `cache_test`, `gaps_*`, `gaps2_*` — run against real PG+Redis (Docker).

## Frontend (web-ui/)

- React 19, Vite 8, Tailwind CSS 4, TypeScript 7.
- State: Zustand (client) + TanStack Query v5 (server).
- HTTP: `ky` (beforeRequest sends `Accept-Language`, beforeError parses localized error body). Routing: `react-router-dom` v7. Upload: `react-dropzone`. Toasts: `sonner`. i18n: i18next ^26 + react-i18next ^17 (see 国际化 (i18n) section).
- Entry: `src/main.tsx` → `App.tsx`. Dev server :5173, proxy to :3000.
- **CSS variables**: Design system uses `var(--color-*)` tokens for theming. Glass effects via `backdrop-blur-sm`, `bg-[var(--glass-bg)]`, `border-[var(--color-border)]`.
- **Responsive strategy**: mobile-first Tailwind default breakpoints (sm=640/md=768/lg=1024/xl=1280). Global `overflow-x: clip` guard on html/body. `<md`: hamburger nav (MobileNav drawer), category filter via Sheet drawer, bottom-sheet dialogs, cardified admin tables; ≥md: desktop sidebar/nav/dialogs preserved.
- **Shared overlay primitives**: `hooks/useOverlay(onClose, enabled)` (Escape close + body scroll lock + overlay click, gated on `enabled`), `ui/Modal` (bottom-sheet <sm / centered ≥sm, `role="dialog"`, panel keeps `.glass-modal` class — E2E locates dialogs by it), `ui/ConfirmDialog` (danger confirm), `ui/Sheet` (left slide-in drawer), `ui/DropdownMenu`/`ui/GlassSelect` (viewport-clamped via `clampLeft`).
- **Hooks**: `useUploadQueue` (multi-file upload with concurrency pool), `useInfiniteQuery` (Gallery scroll), `useFormat` (locale-aware formatBytes/formatDate/formatNumber), `useOverlay` (overlay behavior), `useImageZoom` (pure zoom/pan state — scale/offset, anchored zoom math, range/pan clamping).
- **Components**: `CategoryTree` (sidebar with inline CRUD — context menu + touch ⋯ button, rename, delete confirmation, create modal), `MobileNav` (hamburger drawer), `ImageViewer` (fullscreen zoom viewer — click the image to open; wheel/button/keyboard/pinch zoom, drag pan, double-click fit↔100%), `ui/DropdownMenu` (NavBar user menu), `SystemConfig` (admin config management — test connections, save/restore), `LanguageSwitcher` (NavBar + Login/Register).
- **Mobile E2E**: viewport 375×667 via `test.use({ viewport, hasTouch: true })` in specs (mobile-nav/mobile-gallery/mobile-admin/responsive). Note: `overflow-x: clip` masks `documentElement.scrollWidth` — overflow assertions must measure `Math.max(doc, body)`.

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
