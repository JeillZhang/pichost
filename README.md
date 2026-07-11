# PicHost

Self-hosted image hosting service — multi-user, JWT auth, LocalFS storage, public URL sharing with full-format links (URL/Markdown/HTML/BBCode).

## Tech Stack

| Layer | Stack |
|-------|-------|
| Backend | Rust 1.96, Axum 0.8, Tokio 1.52, sqlx 0.8 |
| Frontend | React 19, Vite 6, TypeScript 5.7, Tailwind CSS 4 |
| Database | PostgreSQL 18 |
| Cache | Redis 8.0 |
| Deployment | Docker Compose |

## Quick Start

```bash
# Clone and enter the repo
git clone <repo-url> && cd pichost-rust

# Start with Docker Compose
docker compose up --build -d

# Register a user
curl -s -X POST http://localhost:3000/api/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123456"}'

# Or open the frontend
open http://localhost:5173
```

## Development

### Prerequisites

- **Rust 1.96+** (via `rustup`)
- **Node.js 22+** and **npm**
- **PostgreSQL 18** and **Redis 8** (or use Docker)

### Setup

```bash
# Backend
cargo build --workspace
cp .env.example .env   # edit PICHOST_AUTH_JWT_SECRET

# Frontend
cd web-ui && npm install && npm run dev
```

### Run

```bash
# Terminal 1 — backend
PICHOST_AUTH_JWT_SECRET=your-secret cargo run -p pichost-api

# Terminal 2 — frontend dev server (proxies /api and /u to localhost:3000)
cd web-ui && npm run dev
```

### Test

```bash
# Rust unit/integration tests
cargo test --workspace

# Frontend type check + build
cd web-ui && npm run build
```

## API Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/api/v1/auth/register` | No | Register user |
| POST | `/api/v1/auth/login` | No | Login |
| POST | `/api/v1/images` | Yes | Upload image |
| GET | `/api/v1/images` | Yes | List images |
| GET | `/api/v1/images/:id` | Yes | Get image details |
| GET | `/u/:public_key` | No | Serve image file (public) |

## Project Structure

```
├── pichost-core/           # Domain models, config, storage trait
├── pichost-api/            # Axum web server (routes, middleware, services)
├── pichost-worker/         # Async worker placeholder (P1)
├── web-ui/                 # React SPA frontend
├── migrations/             # PostgreSQL migrations
├── Dockerfile.api          # Multi-stage Rust build
├── docker-compose.yml      # Full stack deployment
└── .env.example            # Environment variable template
```

## Features (P0)

- [x] User registration & login (Argon2id password hashing)
- [x] JWT-based authentication (HS256, access + refresh tokens)
- [x] Redis token blacklist for logout
- [x] Image upload with drag-and-drop
- [x] File type & magic byte validation
- [x] SHA256 dedup (same user + same content → 200)
- [x] Public image serving at `/u/{public_key}`
- [x] Full-format link output (URL, Markdown, HTML, BBCode)
- [x] Dashboard with recent image list
- [x] Gallery grid view
- [x] Image detail page with 1-click link copy
- [x] File size limits (50 MB admin, 10 MB user)
- [x] Docker Compose deployment

## P1 Plans

- Async worker for thumbnail/webp generation
- RustFS storage backend
- Rate limiting per user/IP
- Admin dashboard
- OAuth login
