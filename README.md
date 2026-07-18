# PicHost

Self-hosted image hosting service — multi-user, JWT auth, OAuth login, local/S3 storage, thumbnails, CDN-ready, Prometheus metrics.

**v0.14.0** — P2 complete. 10 major features, all documented.

## Stack

| Layer | Technology |
|-------|-----------|
| Backend | Rust 1.96+ (Axum 0.8, Tokio, sqlx) |
| Frontend | React 19, Vite 8, TypeScript 7, Tailwind CSS 4 |
| Database | PostgreSQL 18 |
| Cache / Queue | Redis 8 |
| Proxy / CDN | Nginx 1.27 (reverse proxy, cache, rate limiting) |
| Deployment | Docker Compose (API×2, Worker×2, stateless) |

## Quick Start (Docker)

```bash
# 1. Clone and enter the repo
git clone https://github.com/JeillZhang/pichost.git && cd pichost-rust

# 2. Create your .env file
cp .env.example .env
# Edit: the JWT secret MUST be changed from the default
# Minimal required:
#   PICHOST_AUTH_JWT_SECRET=<at-least-32-random-chars>

# 3. Build frontend assets (required for Nginx)
cd web-ui && npm install && npm run build && cd ..

# 4. Start the full stack
docker compose up --build -d

# 5. Register the first user (auto-admin)
curl -s -X POST http://localhost/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123456"}'

# 6. Open the app
open http://localhost
```

The stack runs on **port 80** via Nginx, proxying to 2 API replicas, with 2 background workers.

## Architecture

```
Browser / Client
       │
       ▼
   Nginx :80
   ├── Static files (web-ui/dist)
   ├── Proxy cache (images: 50 MB / 1 hr)
   ├── Rate limiting (API 60r/m, public 200r/m)
   └── Upstream least_conn ──┬── API replica 1 (:3000)
                             └── API replica 2 (:3000)
                                    │
              ┌─────────────────────┼──────────────────┐
              ▼                     ▼                  ▼
         PostgreSQL 18          Redis 8           Local Storage
         (data, quotas)    (cache, queue,       (./storage-local/)
                            rate limits,         or S3 via RustFS
                            token blacklist)
                                    │
                                    ▼
                           Worker queue (BRPOP)
                           ├── Worker 1
                           └── Worker 2
                           (thumbnails, WebP conversion)
```

## Local Development

### Prerequisites

- Rust 1.96+ (`rustup`), Node.js 22+, PostgreSQL 18, Redis 8
- Or: use Docker for PG + Redis (`docker compose up postgres redis`)

### Setup & Run

```bash
# Backend — edit .env first with PICHOST_AUTH_JWT_SECRET (min 32 chars)
cp .env.example .env
cargo build --workspace
PICHOST_AUTH_JWT_SECRET=your-secret cargo run -p pichost-api

# Frontend — proxies /api and /u to localhost:3000
cd web-ui && npm install && npm run dev  # → http://localhost:5173
```

### Test & Lint

```bash
cargo test --workspace                      # 14 pass, 10 ignored (need DB)
cargo clippy --workspace -- -D warnings      # zero warnings required
cd web-ui && npm run build                   # tsc -b && vite build
```

Run a single test: `cargo test -p pichost-api test_image_list`

## Configuration

All config via env vars with `PICHOST_` prefix (figment: defaults → env overrides).

| Variable | Required | Default | Purpose |
|----------|----------|---------|---------|
| `PICHOST_DATABASE_URL` | Yes | — | PostgreSQL connection string |
| `PICHOST_REDIS_URL` | Yes | — | Redis connection string |
| `PICHOST_AUTH_JWT_SECRET` | **Yes** | — | HS256 signing key (min 32 chars) |
| `PICHOST_SERVER_PUBLIC_URL` | Production | `http://localhost` | For OAuth callbacks and share links |
| `PICHOST_AUTH_OAUTH_GITHUB_CLIENT_ID` | OAuth | — | GitHub OAuth App client ID |
| `PICHOST_AUTH_OAUTH_GITHUB_CLIENT_SECRET` | OAuth | — | GitHub OAuth App secret |
| `PICHOST_AUTH_OAUTH_GOOGLE_CLIENT_ID` | OAuth | — | Google OAuth client ID |
| `PICHOST_AUTH_OAUTH_GOOGLE_CLIENT_SECRET` | OAuth | — | Google OAuth client secret |
| `PICHOST_STORAGE_LOCAL_BASE_PATH` | Local storage | `./storage-local` | File storage directory |
| `DATABASE_URL` | Docker only | — | sqlx CLI helper (not consumed by app) |

**Important**: `DATABASE_URL` and `PICHOST_DATABASE_URL` are separate vars. Only `PICHOST_DATABASE_URL` is consumed at runtime. Both are set in docker-compose for convenience.

## API Endpoints

### Auth
| Method | Path | Auth | Notes |
|--------|------|------|-------|
| POST | `/api/v1/auth/register` | No | Invite code required (first user auto-admin) |
| POST | `/api/v1/auth/login` | No | Returns access + refresh tokens |
| POST | `/api/v1/auth/refresh` | Refresh token | |
| POST | `/api/v1/auth/logout` | JWT | Blacklists token in Redis |
| GET | `/api/v1/auth/oauth/github` | No | Redirect to GitHub OAuth |
| GET | `/api/v1/auth/oauth/google` | No | Redirect to Google OAuth |
| GET | `/api/v1/auth/oauth/{provider}/callback` | No | Returns JWT |

### Images
| Method | Path | Auth | Notes |
|--------|------|------|-------|
| POST | `/api/v1/images` | JWT | Multipart upload, auto-thumbnails |
| GET | `/api/v1/images` | JWT | Paginated: `?page=&per_page=&sort=&order=&search=` |
| GET | `/api/v1/images/:id` | JWT | |
| DELETE | `/api/v1/images/:id` | JWT | |
| POST | `/api/v1/images/batch-delete` | JWT | `{ ids: UUID[] }`, max 100 |
| GET | `/u/{public_key}` | No | Public image, cached 1 year |
| GET | `/u/thumb/{id}` | No | Thumbnail variant |
| GET | `/u/webp/{id}` | No | WebP variant |

### User & Admin
| Method | Path | Auth | Notes |
|--------|------|------|-------|
| GET | `/api/v1/users/me/stats` | JWT | Storage usage + quota |
| POST | `/api/v1/users/oauth/link` | JWT | Link OAuth after invite-code registration |
| GET | `/api/v1/admin/stats` | JWT+Admin | System-wide stats |
| GET/POST | `/api/v1/admin/invites` | JWT+Admin | Invite code management |
| GET | `/api/v1/admin/users` | JWT+Admin | Paginated, includes quotas |
| PATCH | `/api/v1/admin/users/:id` | JWT+Admin | Edit user + set `storage_quota` |
| DELETE | `/api/v1/admin/users/:id` | JWT+Admin | Cascades (images, oauth links) |

### Observability
| Method | Path | Auth | Notes |
|--------|------|------|-------|
| GET | `/metrics` | No | Prometheus text format |
| GET | `/health` | No | Nginx health check (also `/api/health` JSON) |

## Features

- [x] User registration — Argon2id password hashing, invite-code gating
- [x] JWT auth — HS256, access (15 min) + refresh (30 days), Redis blacklist
- [x] OAuth login — GitHub & Google OAuth2 (link after invite registration)
- [x] Image upload — drag-and-drop, magic byte validation, per-user SHA256 dedup
- [x] **Storage quota** — per-user limit (default 1 GB, admin adjustable, NULL = unlimited)
- [x] Thumbnails & WebP — async via Redis queue, 2 worker replicas
- [x] Gallery — pagination, search (ILIKE), sort (created_at / file_size / name)
- [x] **Multi-file upload** — concurrent queue (max 3), per-file progress cards
- [x] **Batch management** — delete up to 100 images at once
- [x] Public sharing — `/u/{public_key}` with full-format links (URL/MD/HTML/BBCode)
- [x] Admin panel — user management, invite codes, system stats, quota control
- [x] **Rate limiting** — 4 strategies (auth, upload, general, public), Redis-backed
- [x] Nginx — reverse proxy, proxy_cache, gzip, upstream least_conn
- [x] **Horizontal scaling** — API×2, Worker×2 in docker-compose
- [x] **Prometheus /metrics** — counters (uploads, registrations), gauges (users, images)
- [x] RustFS storage backend — S3-compatible object storage (optional)

## Project Structure

```
├── pichost-core/            Domain models, config, StorageBackend trait,
│                            LocalStorage, RustfsStorage, StorageRouter
├── pichost-api/             Axum server — routes, middleware, services,
│                            DB pool, Redis, rate limiting
├── pichost-worker/          Background processing — thumbnails, WebP via Redis queue
├── web-ui/                  React SPA — Zustand, TanStack Query, Tailwind CSS 4
├── nginx/
│   └── nginx.conf           Reverse proxy + cache + rate limiting
├── migrations/              7 SQL migrations (0001–0007)
├── Dockerfile.api           Multi-stage Rust build for API
├── Dockerfile.worker        Multi-stage Rust build for Worker
├── docker-compose.yml       Full stack: Nginx, API×2, Worker×2, PostgreSQL, Redis
├── .env.example             Environment variable template
└── docs/
    ├── superpowers/specs/   Design doc (2026-07-11-pichost-design.md)
    └── superpowers/guides/  CDN setup guide, architecture notes
```

## Deployment

### Docker (recommended)

```bash
# Build front-end first (Nginx serves it as static files)
cd web-ui && npm run build && cd ..

# Start full stack
docker compose up --build -d

# Verify
curl http://localhost/health
```

Default compute layout: 2 API replicas (least_conn), 2 worker replicas (independent consumers). Postgres and Redis ports are **not exposed** to the host — internal Docker network only.

### Production checklist

1. **Change `PICHOST_AUTH_JWT_SECRET`** — never use the default.
2. Set `PICHOST_SERVER_PUBLIC_URL` to your real domain (for OAuth callbacks, share links).
3. **Use a volume or S3 backend for storage** — the default `./storage-local` loses data when containers are destroyed.
4. Configure OAuth credentials (GitHub/Google) if you want OAuth login.
5. **Put a CDN in front of Nginx** — see `docs/superpowers/guides/cdn-setup.md`.
6. Scale `deploy.replicas` in docker-compose.yml as needed.

### Volume management

```bash
docker compose down       # Stop containers (keep data)
docker compose down -v    # Wipe PostgreSQL + Redis data
```

### Check logs

```bash
docker compose logs api     # API replicas
docker compose logs worker  # Background workers
docker compose logs nginx   # Proxy requests
```

## Troubleshooting

| Symptom | Likely cause | Fix |
|---------|-------------|-----|
| 401 on all requests | Redis down | `docker compose ps redis` — blacklist fails closed |
| 413 on upload | Storage quota exceeded | Admin: increase user quota or set to NULL |
| Nginx returns 502 | API not ready yet | Wait ~5s for migrations to finish |
| Frontend blank at `localhost` | `web-ui/dist` missing | `cd web-ui && npm run build` |
| Docker build fails | Dockerfile.api needs `COPY` context | Run from repo root |
| `npm run build` fails | Node.js < 22 | Check `node -v` (need 22+) |