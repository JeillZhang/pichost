# PicHost — Agent Guide

## Workspace

- Cargo workspace: `pichost-core`, `pichost-api`, `pichost-worker`.
- `pichost-worker` is **placeholder only** (`fn main() { println!(...) }`). Ignore for P0 work.
- Rust edition 2021, stable toolchain with `rustfmt` + `clippy` (see `rust-toolchain.toml`). No custom fmt/clippy config.
- No `rust-analyzer` / LSP override needed — standard setup works.

## Key Commands

| Action | Command | Notes |
|---|---|---|
| Build all | `cargo build --workspace` | |
| Run API server | `cargo run -p pichost-api` | Requires DB + Redis, see setup below |
| Test all | `cargo test --workspace` | Only `pichost-core` has tests (3 integration tests in `tests/storage_test.rs`) |
| Frontend dev | `cd web-ui && npm run dev` | Vite proxies `/api` and `/u` → `localhost:3000` |
| Frontend build | `cd web-ui && npm run build` | Runs `tsc -b && vite build` (type-check first, then bundle) |
| Docker full stack | `docker compose up --build -d` | Postgres + Redis + API |

## Setup Gotchas

- **Copy `.env.example → .env`, then edit `PICHOST_AUTH_JWT_SECRET`.** `.env` is gitignored.
- **Two DB URL env vars exist:** `DATABASE_URL` (sqlx CLI helper, not consumed by app) and `PICHOST_DATABASE_URL` (consumed by figment config). Both must be set for docker-compose; for local dev only `PICHOST_DATABASE_URL` matters at runtime.
- **sqlx queries are runtime-only** (uses `query_as`, `query_scalar` — no `query!` macro). No compile-time DB needed, no `sqlx prepare` step.
- `storage-local/` is gitignored and created at runtime by LocalStorage. No manual setup needed.
- Prerequisites: Rust 1.96+, Node.js 22+, PostgreSQL 18, Redis 8.

## Config System

- Uses `figment` crate: defaults → `config.toml` (optional file) → `PICHOST_*` env vars override.
- All env vars use prefix `PICHOST_` (e.g. `PICHOST_DATABASE_URL`, `PICHOST_AUTH_JWT_SECRET`).
- Config struct defined in `pichost-core/src/config.rs` — has `Default` impl with dev defaults.
- No `config.toml` exists in the repo; env vars are the intended override mechanism.

## Architecture Notes

- **Auth**: JWT HS256 via `jsonwebtoken`. Access token TTL = 900s (15 min), refresh token TTL = 2,592,000s (30 days).
- **Redis**: Token blacklist uses key format `bl:{user_id}`. Blacklist check **fails closed** (`unwrap_or(true)`) — if Redis is down, auth always fails.
- **Upload flow**: Multipart → magic byte check (`infer::is_image`) → SHA256 hash → per-user dedup check → random 6-char hex public key (collision loop) → write to LocalStorage at `{base_path}/{user_id}/{public_key}` → INSERT with status `'active'`.
- **Dedup**: Per-user, per-SHA256. Same user uploading identical content → returns existing image metadata with 200 (not an error). Different users uploading the same content → separate entries.
- **Public serving**: `GET /u/{public_key}` with `Cache-Control: public, max-age=31536000, immutable`. Checks `status = 'active'` — images with other statuses return 404.
- **Image status quirk**: DB default is `'pending'`, but upload INSERT hardcodes `'active'`. The `ImageStatus` enum has `Pending/Processing/Ready/Failed` variants but the code checks against the string `"active"`. If you add status transitions, reconcile this.

## CRATE BOUNDARIES

- **pichost-core** (`pichost_core`): Domain models, config, error types, `StorageBackend` trait + `LocalStorage` impl. No web/framework deps.
- **pichost-api** (`pichost_api`): Axum server — routes, middleware, services, DB pool, Redis cache. Depends on `pichost-core`.
- **pichost-worker**: Placeholder binary. Depends on `pichost-core`.

## Testing

- Only `pichost-core` has tests: 3 integration tests under `tests/storage_test.rs` using `tempfile::TempDir`.
- No unit tests, no `pichost-api` tests, no web-ui tests.
- Integration tests need `tokio` features `["rt", "macros"]` (configured in dev-dependencies).
- Adding tests to api/routes would require DB + Redis.

## Frontend (web-ui/)

- React 19, Vite 6, Tailwind CSS 4, TypeScript 7.
- State: Zustand (client state) + TanStack Query (server state).
- HTTP client: `ky` (not axios, not fetch).
- Routing: react-router-dom v7.
- Upload: react-dropzone.
- Toasts: sonner.
- Entrypoint: `src/main.tsx` → `App.tsx`.
- Dev server on `:5173` with proxy to `:3000`.

## Projects that are NOT present

- No CI workflows (no `.github/workflows/`).
- No pre-commit hooks.
- No lint/staging config.
- No `deny.toml` (no cargo-deny).
- No Makefile or Justfile.
- No `opencode.json`.

## Rules

- 当一个阶段Plan开发完成时，自动清理生成的临时文件（如 log 文件等）、临时Docker容器等，避免垃圾残留。
- 当一条命令卡主超过120s时，自动取消重试，避免任务进度阻塞。
- docs 目录下的文件也是当前代码仓的重要交付件，自动纳入版本控制管理。
- 编写的 Rust 代码需要符合 Rust 编码规范，如果有规范，请进行修复，确保清零。
- Rust 代码开发遵循 TDD（测试驱动开发），先编写完整单元测试 UT 用例后，再进行开发，确保用例通过。
- 所有代码编程完成以后，需进行系统集成冒烟测试，确保业务符合功能设计。
- 每完成一个Plan的开发，整理更新 `docs\superpowers\specs\2026-07-11-pichost-design.md` **开发优先级** 部分的 `TODO` 项，
  避免每次重复分析待开发功能。
- commit 时，描述信息及标题统一使用英文，`docs\superpowers` 下的文档统一使用中文。
