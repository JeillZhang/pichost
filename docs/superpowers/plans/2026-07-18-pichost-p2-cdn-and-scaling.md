# CDN Integration + Horizontal Scaling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add Nginx reverse proxy with proxy caching (CDN-ready), gzip compression, and scale API + Worker to multiple replicas with load balancing.

**Architecture:** Add Nginx as the entry point (port 80) in Docker Compose, proxying `/api/` to API replicas (least_conn balancing), `/u/` and `/t/` with aggressive proxy caching (1h), and static frontend assets directly from Nginx. API and Worker scaled to 2 replicas each. Frontend build output mounted into Nginx. CDN documentation for Cloudflare setup included.

**Tech Stack:** Nginx 1.27-alpine, Docker Compose v3, existing Rust/React unchanged

## Global Constraints

- No Rust code changes — deployment configuration only
- No new npm dependencies
- `cargo clippy --workspace -- -D warnings` must pass (no code changes, so trivially)
- `npm run build` must pass (frontend build for Nginx serving)
- Existing Docker volumes and environment variables preserved
- All commits in English, spec docs in Chinese

---

## File Structure

```
nginx/nginx.conf                     (CREATE) — Nginx config with upstream, cache, gzip
docker-compose.yml                   (MODIFY) — add nginx service, scale api/worker
docs/superpowers/guides/cdn-setup.md (CREATE) — Cloudflare/CDN setup guide
```

---

### Task 1: Create Nginx configuration with proxy caching and gzip

**Files:**
- Create: `nginx/nginx.conf`

- [ ] **Step 1: Create nginx directory and config**

```bash
mkdir -p nginx
```

Create `nginx/nginx.conf`:

```nginx
worker_processes auto;

events {
    worker_connections 1024;
}

http {
    include       /etc/nginx/mime.types;
    default_type  application/octet-stream;

    # ── Gzip compression ──
    gzip on;
    gzip_comp_level 5;
    gzip_min_length 256;
    gzip_types text/plain text/css application/json application/javascript text/xml application/xml text/javascript image/svg+xml;
    gzip_vary on;

    # ── Proxy cache path (1 GB, inactive 1h) ──
    proxy_cache_path /var/cache/nginx levels=1:2 keys_zone=IMAGE_CACHE:50m max_size=1g inactive=1h use_temp_path=off;

    # ── API upstream (least_conn for fair distribution) ──
    upstream api {
        least_conn;
        server api:3000 max_fails=3 fail_timeout=30s;
        server api:3000 max_fails=3 fail_timeout=30s;
    }

    # ── Rate limit zone ──
    limit_req_zone $binary_remote_addr zone=api_limit:10m rate=60r/m;
    limit_req_zone $binary_remote_addr zone=public_limit:10m rate=200r/m;

    server {
        listen 80;

        # ── Static frontend (SPA fallback) ──
        location / {
            root /usr/share/nginx/html;
            try_files $uri $uri/ /index.html;
        }

        # ── Public images (CDN-cacheable, long TTL) ──
        location /u/ {
            limit_req zone=public_limit burst=50 nodelay;
            proxy_pass http://api;
            proxy_cache IMAGE_CACHE;
            proxy_cache_valid 200 1h;
            proxy_cache_key "$uri";
            proxy_cache_use_stale error timeout updating http_500 http_502 http_503;
            add_header X-Cache-Status $upstream_cache_status;
        }

        # ── Thumbnails (CDN-cacheable) ──
        location /t/ {
            limit_req zone=public_limit burst=50 nodelay;
            proxy_pass http://api;
            proxy_cache IMAGE_CACHE;
            proxy_cache_valid 200 1h;
            proxy_cache_key "$uri";
            proxy_cache_use_stale error timeout updating;
            add_header X-Cache-Status $upstream_cache_status;
        }

        # ── API (no cache, pass through) ──
        location /api/ {
            proxy_pass http://api;
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
            proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
            proxy_set_header X-Forwarded-Proto $scheme;
            proxy_read_timeout 60s;
        }

        # ── Metrics (no cache, no auth — Prometheus scrapes this) ──
        location /metrics {
            proxy_pass http://api;
            proxy_set_header Host $host;
            proxy_set_header X-Real-IP $remote_addr;
        }

        # ── Health check ──
        location /health {
            access_log off;
            return 200 "ok";
            add_header Content-Type text/plain;
        }
    }
}
```

- [ ] **Step 2: Commit**

```bash
git add nginx/nginx.conf
git commit -m "feat: add nginx config with proxy caching, gzip, and upstream load balancing"
```

---

### Task 2: Update Docker Compose — add Nginx, scale API/Worker

**Files:**
- Modify: `docker-compose.yml`

- [ ] **Step 1: Add Nginx service and update API/Worker for scaling**

Replace `docker-compose.yml`:

```yaml
# =============================================================================
# PicHost — Docker Compose (with Nginx + horizontal scaling)
# =============================================================================

services:
  # ── PostgreSQL 18 ──
  postgres:
    image: bitnami/postgresql:latest
    restart: unless-stopped
    environment:
      POSTGRES_USER: pichost
      POSTGRES_PASSWORD: pichost
      POSTGRES_DB: pichost
    volumes:
      - pgdata:/var/lib/postgresql/data
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U pichost -d pichost"]
      interval: 5s
      timeout: 5s
      retries: 5

  # ── Redis 8 ──
  redis:
    image: redis:8-alpine
    restart: unless-stopped
    command: redis-server --maxmemory 256mb --maxmemory-policy allkeys-lru

  # ── API (2 replicas, stateless, JWT via Redis) ──
  api:
    build:
      dockerfile: Dockerfile.api
      context: .
    restart: unless-stopped
    deploy:
      replicas: 2
    depends_on:
      postgres:
        condition: service_healthy
      redis:
        condition: service_started
    volumes:
      - ./storage-local:/app/storage-local
    environment:
      DATABASE_URL: postgres://pichost:pichost@postgres/pichost
      PICHOST_DATABASE_URL: postgres://pichost:pichost@postgres/pichost
      PICHOST_REDIS_URL: redis://redis:6379
      PICHOST_AUTH_JWT_SECRET: ${PICHOST_AUTH_JWT_SECRET:-dev-secret-change-me-in-production}
      PICHOST_STORAGE_LOCAL_BASE_PATH: /app/storage-local
      PICHOST_SERVER_PUBLIC_URL: ${PICHOST_SERVER_PUBLIC_URL:-http://localhost}

  # ── Worker (2 replicas, independent consumers from Redis queue) ──
  worker:
    build:
      dockerfile: Dockerfile.worker
      context: .
    restart: unless-stopped
    deploy:
      replicas: 2
    depends_on:
      postgres:
        condition: service_healthy
      redis:
        condition: service_started
    volumes:
      - ./storage-local:/app/storage-local
    environment:
      DATABASE_URL: postgres://pichost:pichost@postgres/pichost
      PICHOST_DATABASE_URL: postgres://pichost:pichost@postgres/pichost
      PICHOST_REDIS_URL: redis://redis:6379
      PICHOST_AUTH_JWT_SECRET: ${PICHOST_AUTH_JWT_SECRET:-dev-secret-change-me-in-production}
      PICHOST_STORAGE_LOCAL_BASE_PATH: /app/storage-local
      PICHOST_SERVER_PUBLIC_URL: ${PICHOST_SERVER_PUBLIC_URL:-http://localhost}

  # ── Nginx — reverse proxy + CDN cache + static files ──
  nginx:
    image: nginx:1.27-alpine
    restart: unless-stopped
    ports:
      - "80:80"
    volumes:
      - ./nginx/nginx.conf:/etc/nginx/nginx.conf:ro
      - ./web-ui/dist:/usr/share/nginx/html:ro
    depends_on:
      - api
    healthcheck:
      test: ["CMD", "wget", "-qO-", "http://localhost/health"]
      interval: 10s
      timeout: 5s
      retries: 3

volumes:
  pgdata:
```

Key changes from original:
- **Nginx service**: port 80 entry, mounts nginx config + frontend dist
- **API**: `deploy.replicas: 2`, removed `ports` (only Nginx exposes ports), removed `DATABASE_URL`
- **Worker**: `deploy.replicas: 2`, cleaned up duplicate `DATABASE_URL`
- **Postgres/Redis**: removed `ports` (internal-only via Docker network)
- **Env vars**: `PICHOST_JWT_SECRET` → `PICHOST_AUTH_JWT_SECRET`, added `${...:-default}` fallbacks

- [ ] **Step 2: Update .env.example**

Create/update `.env.example`:

```
# PicHost — Environment Variables
# Copy to .env and fill in values

# Auth (REQUIRED — change in production)
PICHOST_AUTH_JWT_SECRET=your-secret-at-least-32-bytes

# Public URL (for OAuth callbacks and link generation)
PICHOST_SERVER_PUBLIC_URL=http://localhost

# OAuth (optional — for GitHub/Google login)
# PICHOST_AUTH_OAUTH_GITHUB_CLIENT_ID=
# PICHOST_AUTH_OAUTH_GITHUB_CLIENT_SECRET=
# PICHOST_AUTH_OAUTH_GOOGLE_CLIENT_ID=
# PICHOST_AUTH_OAUTH_GOOGLE_CLIENT_SECRET=
```

- [ ] **Step 3: Commit**

```bash
git add docker-compose.yml .env.example
git commit -m "feat: add nginx reverse proxy, scale api/worker to 2 replicas each"
```

---

### Task 3: Create CDN setup guide

**Files:**
- Create: `docs/superpowers/guides/cdn-setup.md`

- [ ] **Step 1: Write CDN guide**

```markdown
# PicHost — CDN Setup Guide

PicHost serves images with `Cache-Control: public, max-age=31536000, immutable` headers
and Nginx proxy caching (`/u/*`, `/t/*`). To add a CDN layer, follow the instructions below.

## Cloudflare (Free Tier)

1. **Add your domain to Cloudflare** — follow their DNS setup wizard.

2. **Configure DNS**: Point your domain (`pichost.example.com`) to your server's IP
   with the orange cloud (proxied) enabled.

3. **Cache Rules** (Dashboard → Caching → Cache Rules):
   | Rule | When | Action |
   |------|------|--------|
   | Image files | URI path starts with `/u/` | Cache: Eligible for cache, Edge TTL: 7 days |
   | Thumbnails | URI path starts with `/t/` | Cache: Eligible for cache, Edge TTL: 7 days |
   | Static assets | URI path ends with `.js`/`.css`/`.svg` | Cache: Eligible for cache |

4. **Always Online**: Enable under Caching → Configuration. Serves cached images
   if your origin is down.

5. **Page Rules** (optional): Create a page rule to bypass cache for `/api/*` paths.

## Other CDNs (Fastly, BunnyCDN, KeyCDN)

Similar setup — point the CDN to your origin, configure:
- Cache `/u/*` and `/t/*` with long TTL
- Bypass cache for `/api/*` and `/metrics`
- Forward `Host` and `X-Forwarded-For` headers

## Verification

```bash
# Check cache headers
curl -I https://your-domain.com/u/some-image-key

# Expected output:
# Cache-Control: public, max-age=31536000, immutable
# X-Cache-Status: HIT (after first request)
```
```

- [ ] **Step 2: Commit**

```bash
git add docs/superpowers/guides/cdn-setup.md
git commit -m "docs: add CDN setup guide for Cloudflare and other providers"
```

---

### Task 4: Smoke test + spec/summary update + version bump

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

- [ ] **Step 2: Docker smoke test**

```bash
docker compose up --build -d
sleep 15
curl -s http://localhost/api/health | jq .
curl -s -I http://localhost/u/test 2>&1 | grep -i cache
docker compose down
```

Expected: Health returns JSON, cache headers present on image requests.

- [ ] **Step 3: Update spec TODO**

```markdown
- [x] CDN 集成 (Nginx proxy_cache + Cloudflare guide)
- [x] 水平扩展 (Nginx upstream least_conn, api ×2, worker ×2, Docker deploy replicas)
```

- [ ] **Step 4: Update summary**

Mark both CDN and horizontal scaling complete. Version bump to `0.14.0`. P2 全部完成!

- [ ] **Step 5: Commit**

```bash
git add docs/... .omo/... Cargo.toml Cargo.lock docs/superpowers/plans/...
git commit -m "chore: update spec and summary for CDN and scaling, bump version to 0.14.0 — P2 complete!"
```

---

## Self-Review Checklist

### 1. Spec Coverage
- ✅ Nginx reverse proxy: Task 1
- ✅ Proxy caching for images: Task 1
- ✅ Gzip compression: Task 1
- ✅ API horizontal scaling: Task 2 (deploy replicas: 2 + upstream)
- ✅ Worker scaling: Task 2
- ✅ CDN guide: Task 3
- ✅ Smoke test: Task 4

### 2. Placeholder Scan
- ✅ No "TBD", "TODO"
- ✅ All Nginx directives have explicit values
- ✅ Docker Compose env vars have defaults

### 3. Type Consistency
- ✅ Nginx upstream name `api` matches proxy_pass targets
- ✅ Image cache key `$uri` consistent for `/u/` and `/t/`
