# Changelog

All notable changes to PicHost are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

Versions before 0.7.0 are not documented here.

## [0.20.0] - 2026-08-09

### Added

- Image detail zoom viewer: click the image to open a fullscreen lightbox with cursor-anchored wheel zoom, drag pan, double-click fit↔100% toggle, two-finger pinch / single-finger drag on touch devices, toolbar zoom in/out/reset buttons with percentage display, and keyboard `+`/`-`/`0` shortcuts.

## [0.19.1] - 2026-08-09

### Fixed

- Thumbnails/WebP variants not displaying (placeholder shown instead): worker stored variant URLs as `/u/thumb-{id}` / `/u/webp-{id}`, but the routes are `/u/thumb/{id}` / `/u/webp/{id}` — every variant request 404'd. Worker now writes the correct slash-separated URLs.
- PNG thumbnails served with `image/jpeg` Content-Type (thumb keys carry no extension, so key-based MIME guessing was wrong). With `X-Content-Type-Options: nosniff`, browsers refused to render them — the serving handler now detects the MIME from the thumbnail bytes.

## [0.19.0] - 2026-08-08

### Added

- Responsive web layout: mobile hamburger navigation (MobileNav drawer), category filter drawer (Sheet) on mobile, touch-friendly category menu (⋯ button), shared Modal/ConfirmDialog components with mobile bottom-sheet behavior, admin table card-ification on small screens, global horizontal-overflow guard, responsive gallery grid (2/3/3/4/5 columns), settings grid/wrapping fixes, DropZone touch tap area, popover viewport clamping.

## [0.17.5] - 2026-08-02

### Added

- PR-triggered API smoke test workflow (`.github/workflows/smoke-test.yml`): full 555-test suite against PG + Redis + MinIO service containers with a clippy gate (see `docs/superpowers/specs/2026-08-02-pichost-smoke-test-design.md`).

### Changed

- Test coverage phase completion: `cargo llvm-cov --workspace` reaches 91.56% line coverage.

## [0.17.4] - 2026-08-02

### Added

- Comprehensive test suites:
  - API integration tests for all route handlers against real PG + Redis (`pichost-api/tests/`).
  - In-crate unit tests for routes, middleware and services.
  - Worker pipeline unit and integration tests.
  - Core crate unit tests (including S3 not-found detection fix).

### Changed

- Router assembly moved to `pichost-api/src/app.rs` so integration tests exercise the exact production routing.

## [0.17.3] - 2026-08-01

### Added

- **Client-side preprocessing** — browser-side image pipeline (EXIF strip, resize, format convert, compress, rotate) via Web Worker; `PreprocessingSettings` component and dashboard status bar.
- **Server-side watermark** — configurable text overlay (font/color/position/rotation/scale/tile) applied in the Worker pipeline; `watermark_config` JSONB column (migration `0010`); `WatermarkSettings` component.
- **File rename** — inline rename on ImageDetail page, `PATCH /api/v1/images/:id`.
- **Software packaging** — systemd units + `scripts/install.sh` / `scripts/uninstall.sh`, GitHub Actions release CI (`v*` tags → `.tar.gz` artifact + GitHub Release).
- **System config management** — admin config API (GET/PUT with auto-backup/test connections/backup/restore), `config.toml` read/write, `SystemConfig` component in the Admin page.
- **Settings UI optimization** — NavBar user dropdown (Settings/Admin/Logout), accordion settings with hash-based section expand (`#settings?section=...`).
- Configurable rate limits (`PICHOST_RATE_LIMIT_*` env overrides).
- Full Playwright E2E suite over all features + PR CI workflow (`e2e.yml`).
- Glassmorphism UI overhaul; generated asset links merged into the image detail card.

### Fixed

- Stop refresh loop on legitimate 401s; make gallery select reachable.
- Metadata cache invalidation on image writes; upload response and dedup query.
- Allow clearing an image's category back to `None`.
- Admin stats double-counting; quota-based storage bars; partial config saves.

## [0.16.0] - 2026-07-19

### Added

- **Gallery categories** — 2-level hierarchy (`categories` table, migration `0009`), category CRUD API, image move / batch-move endpoints, `?category_id=` gallery filter.
- `CategoryTree` sidebar component with inline CRUD (context menu, rename, delete confirmation, create modal).
- Category assignment dropdown on the ImageDetail page.
- E2E browser smoke test (Playwright) wired into the feature-dev pipeline.

## [0.15.1] - 2026-07-19

### Fixed

- Review findings C1–C3 + I1/I4: Git storage path handling, repo verification, 413 quota errors, max storage configs per user, parallel dual-backend upload.

## [0.15.0] - 2026-07-19

### Added

- **Git storage backend** — push files to GitHub/GitCode via Contents REST API; AES-256-GCM token encryption (`PICHOST_TOKEN_ENCRYPTION_KEY`).
- `user_storage_configs` table (migration `0008`) + storage config CRUD API (`/users/me/storage-configs`).
- Dynamic backend routing via `StorageRouter` (RwLock-backed cache, eviction on config change).
- **Multi-backend upload** — storage target selector per upload, parallel dual-backend write, `storage_config_id` filter on gallery queries.
- Storage config management section in the Settings page.

## [0.14.0] - 2026-07-18

### Added

- **Nginx reverse proxy** — proxy cache (50 MB / 1 h on `/u/` and `/t/`), gzip, upstream `least_conn` load balancing.
- **Horizontal scaling** — 2 API replicas + 2 worker replicas in docker-compose.
- CDN setup guide (`docs/superpowers/guides/cdn-setup.md`).

### Fixed

- Docker build: add `pkg-config` and `libssl-dev` for `openssl-sys`; switch to `trixie-slim` images; Redis healthcheck.
- Figment env var parsing with `.split("_")`.

## [0.13.0] - 2026-07-18

### Added

- **OAuth login** — GitHub & Google OAuth2 via `oauth2` crate; `oauth_accounts` table (migration `0007`).
- OAuth account linking endpoint (`POST /users/oauth/link`) — users register via invite code first, then link OAuth in Settings.
- OAuth account section in the Settings page.

## [0.12.0] - 2026-07-18

### Added

- **Prometheus metrics** — `prometheus` crate + metrics registry, HTTP metrics middleware, `GET /metrics` endpoint.
- Business metrics: uploads, registrations, users, images, storage.

## [0.11.0] - 2026-07-18

### Added

- **Batch management** — `POST /images/batch-delete` endpoint (max 100), bulk selection and batch delete in the Gallery.

## [0.10.0] - 2026-07-18

### Added

- **Storage quota** — `storage_quota` column (migration `0006`), enforcement in the upload pipeline (413 on exceed), admin quota control, storage usage bar on the Dashboard, `storage_quota_default` config.

## [0.9.0] - 2026-07-18

### Added

- **Multi-file upload** — `useUploadQueue` hook (concurrent pool, max 3), multi-select drag-and-drop, `UploadCard` per-file progress and status.
- Prevent state updates after component unmount in the upload queue.

## [0.8.0] - 2026-07-18

### Added

- **Gallery enhancement** — pagination (`page`/`per_page`, max 100), search (ILIKE), sort (`created_at`/`file_size`/`original_name`), infinite scroll.
- Composite index on `images(user_id, original_name)` for filename search.
- `SearchBar` and `SortDropdown` components.

## [0.7.0] - 2026-07-18

### Added

- **Invite code registration system** — invite-code gating (first user auto-admin), admin invite management.
- Admin panel basics — middleware layering, system stats.
- Versioning rule in AGENTS.md.

[0.7.0]: https://github.com/JeillZhang/pichost/releases/tag/v0.7.0
[0.8.0]: https://github.com/JeillZhang/pichost/releases/tag/v0.8.0
[0.9.0]: https://github.com/JeillZhang/pichost/releases/tag/v0.9.0
[0.10.0]: https://github.com/JeillZhang/pichost/releases/tag/v0.10.0
[0.11.0]: https://github.com/JeillZhang/pichost/releases/tag/v0.11.0
[0.12.0]: https://github.com/JeillZhang/pichost/releases/tag/v0.12.0
[0.13.0]: https://github.com/JeillZhang/pichost/releases/tag/v0.13.0
[0.14.0]: https://github.com/JeillZhang/pichost/releases/tag/v0.14.0
[0.15.0]: https://github.com/JeillZhang/pichost/releases/tag/v0.15.0
[0.15.1]: https://github.com/JeillZhang/pichost/releases/tag/v0.15.1
[0.16.0]: https://github.com/JeillZhang/pichost/releases/tag/v0.16.0
[0.17.3]: https://github.com/JeillZhang/pichost/releases/tag/v0.17.3
[0.17.4]: https://github.com/JeillZhang/pichost/releases/tag/v0.17.4
[0.17.5]: https://github.com/JeillZhang/pichost/releases/tag/v0.17.5
