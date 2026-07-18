# Prometheus /metrics Endpoint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose a `GET /metrics` endpoint in Prometheus text format with HTTP counters, latencies, and business metrics (uploads, storage, users).

**Architecture:** Add `prometheus` crate to `pichost-api`. Create a metrics module with a global registry and gauge/counter/histogram definitions. Add an Axum metrics middleware for per-route request counts and latencies. Expose a `/metrics` route that renders all registered metrics. Integrate business metrics into existing handlers (upload counter increment, user count gauge update).

**Tech Stack:** Rust 1.96, Axum 0.8, `prometheus` crate (official), `lazy_static` or `once_cell` for global registry

## Global Constraints

- Rust edition 2021, `cargo clippy --workspace -- -D warnings` must pass, `cargo test --workspace` must pass
- Frontend unchanged — no new npm dependencies, `npm run build` must still pass
- New Rust crate: `prometheus = "0.13"` — no other new deps
- Metrics endpoint at `GET /metrics` — no authentication required (standard Prometheus convention, use a firewall/reverse proxy for access control)
- Follow existing code patterns: Axum State + Extension pattern, tracing for logs
- All commits in English, spec docs in Chinese

---

## File Structure

```
pichost-api/Cargo.toml                        (MODIFY) — add prometheus dep
pichost-api/src/metrics/mod.rs                 (CREATE) — global registry + metric definitions
pichost-api/src/middleware/metrics.rs          (CREATE) — HTTP metrics middleware
pichost-api/src/main.rs                        (MODIFY) — mount /metrics route + middleware
pichost-api/src/routes/images.rs               (MODIFY) — increment upload counter
pichost-api/src/routes/admin.rs                (MODIFY) — expose user/image count gauges
```

---

### Task 1: Add `prometheus` crate + metrics registry module

**Files:**
- Modify: `pichost-api/Cargo.toml`
- Create: `pichost-api/src/metrics/mod.rs`

**Interfaces:**
- Produces: `registry() -> &'static Registry`, counter/histogram/gauge macros
- Consumed by: Tasks 2-4

- [ ] **Step 1: Add dependency**

```toml
# pichost-api/Cargo.toml
prometheus = "0.13"
lazy_static = "1.5"
```

- [ ] **Step 2: Create metrics module**

```rust
// pichost-api/src/metrics/mod.rs
use lazy_static::lazy_static;
use prometheus::{register_counter_vec, register_histogram_vec, register_int_gauge, Encoder, Registry, TextEncoder};

lazy_static! {
    pub static ref REGISTRY: Registry = Registry::new();

    // HTTP metrics
    pub static ref HTTP_REQUESTS_TOTAL: prometheus::CounterVec = register_counter_vec!(
        "pichost_http_requests_total",
        "Total HTTP requests",
        &["method", "path", "status"]
    )
    .unwrap();

    pub static ref HTTP_REQUEST_DURATION: prometheus::HistogramVec = register_histogram_vec!(
        "pichost_http_request_duration_seconds",
        "HTTP request duration in seconds",
        &["method", "path"],
        vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0]
    )
    .unwrap();

    // Business metrics
    pub static ref UPLOADS_TOTAL: prometheus::Counter = prometheus::register_counter!(
        "pichost_uploads_total",
        "Total image uploads"
    )
    .unwrap();

    pub static ref UPLOAD_ERRORS_TOTAL: prometheus::Counter = prometheus::register_counter!(
        "pichost_upload_errors_total",
        "Total upload errors"
    )
    .unwrap();

    pub static ref TOTAL_USERS: prometheus::IntGauge = register_int_gauge!(
        "pichost_users_total",
        "Total registered users"
    )
    .unwrap();

    pub static ref TOTAL_IMAGES: prometheus::IntGauge = register_int_gauge!(
        "pichost_images_total",
        "Total images"
    )
    .unwrap();

    pub static ref TOTAL_STORAGE_BYTES: prometheus::IntGauge = register_int_gauge!(
        "pichost_storage_bytes_total",
        "Total storage used in bytes"
    )
    .unwrap();
}

pub fn encode_metrics() -> Result<String, prometheus::Error> {
    let encoder = TextEncoder::new();
    let metric_families = REGISTRY.gather();
    let mut buffer = vec![];
    encoder.encode(&metric_families, &mut buffer)?;
    String::from_utf8(buffer).map_err(|e| prometheus::Error::Msg(e.to_string()))
}
```

- [ ] **Step 3: Declare module**

Add to `pichost-api/src/main.rs` or `lib.rs`:
```rust
pub mod metrics;
```

- [ ] **Step 4: Verify compilation**

```bash
cargo check -p pichost-api
```

Expected: PASS (new crates resolve)

- [ ] **Step 5: Commit**

```bash
git add pichost-api/Cargo.toml Cargo.lock pichost-api/src/metrics/mod.rs
git commit -m "feat: add prometheus crate and metrics registry module"
```

---

### Task 2: HTTP metrics middleware + /metrics route

**Files:**
- Create: `pichost-api/src/middleware/metrics.rs`
- Modify: `pichost-api/src/main.rs`

**Interfaces:**
- Consumes: `REGISTRY`, `HTTP_REQUESTS_TOTAL`, `HTTP_REQUEST_DURATION` from Task 1
- Produces: `track_metrics()` middleware function, `/metrics` route
- Consumed by: Task 3 (business metrics integration)

- [ ] **Step 1: Create middleware**

```rust
// pichost-api/src/middleware/metrics.rs
use std::time::Instant;
use axum::{
    extract::Request,
    middleware::Next,
    response::Response,
};
use crate::metrics::{HTTP_REQUESTS_TOTAL, HTTP_REQUEST_DURATION};

pub async fn track_metrics(req: Request, next: Next) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let start = Instant::now();

    let response = next.run(req).await;

    let status = response.status().as_u16().to_string();
    let duration = start.elapsed().as_secs_f64();

    HTTP_REQUESTS_TOTAL
        .with_label_values(&[&method, &path, &status])
        .inc();
    HTTP_REQUEST_DURATION
        .with_label_values(&[&method, &path])
        .observe(duration);

    response
}
```

- [ ] **Step 2: Declare middleware module**

In `main.rs`, add:
```rust
pub mod middleware;
```
(if not already present — check existing middleware module declaration)

Add to `middleware/mod.rs`:
```rust
pub mod metrics;
```

- [ ] **Step 3: Add /metrics route and middleware to app**

In `main.rs`, add the metrics handler and layer:

```rust
pub async fn metrics_handler() -> String {
    metrics::encode_metrics().unwrap_or_else(|e| format!("error: {}", e))
}

// In the app router setup, add:
let app = Router::new()
    // ... existing routes ...
    .route("/metrics", get(metrics_handler))
    .layer(middleware::from_fn(middleware::metrics::track_metrics));
```

NOTE: Apply the metrics middleware as a **layer on the full app** (not on individual routes) so it tracks all requests. Place it AFTER the rate-limiter layers but BEFORE the auth-required route groups.

- [ ] **Step 4: Verify compilation and tests**

```bash
cargo check -p pichost-api
cargo clippy --workspace -- -D warnings
```

Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add pichost-api/src/middleware/metrics.rs pichost-api/src/main.rs
git commit -m "feat: add HTTP metrics middleware and GET /metrics endpoint"
```

---

### Task 3: Business metrics integration

**Files:**
- Modify: `pichost-api/src/routes/images.rs` (upload handler — increment counters)
- Modify: `pichost-api/src/routes/admin.rs` (admin stats — update gauges)

**Interfaces:**
- Consumes: `UPLOADS_TOTAL`, `UPLOAD_ERRORS_TOTAL` from Task 1
- Produces: Uploads counted, business gauges updated on admin stats query

- [ ] **Step 1: Increment upload counters in images.rs**

In the upload handler (`upload_handler`), after successful upload:
```rust
crate::metrics::UPLOADS_TOTAL.inc();
```

In the upload handler's error path (the `Err` branch), before returning the error:
```rust
crate::metrics::UPLOAD_ERRORS_TOTAL.inc();
```

- [ ] **Step 2: Update business gauges in admin.rs**

In the `get_admin_stats` handler, after computing stats from DB, update gauges:

```rust
use crate::metrics::{TOTAL_USERS, TOTAL_IMAGES, TOTAL_STORAGE_BYTES};

// After computing total_users, total_images, total_size:
TOTAL_USERS.set(stats.total_users);
TOTAL_IMAGES.set(stats.total_images);
TOTAL_STORAGE_BYTES.set(stats.total_size);
```

Add this in the cache-miss branch (after the DB query, before returning the response). The cache-hit branch can also update (both paths set the same gauge values).

- [ ] **Step 3: Verify compilation**

```bash
cargo check -p pichost-api
cargo clippy --workspace -- -D warnings
```

Expected: PASS

- [ ] **Step 4: Commit**

```bash
git add pichost-api/src/routes/images.rs pichost-api/src/routes/admin.rs
git commit -m "feat: integrate business metrics (uploads, users, images, storage)"
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

- [ ] **Step 2: Manual /metrics smoke test**

```bash
# Start API server (requires DB + Redis)
PICHOST_AUTH_JWT_SECRET=test cargo run -p pichost-api &
sleep 3
curl -s http://localhost:3000/metrics | head -20
kill %1
```

Expected: Output includes `pichost_http_requests_total`, `pichost_http_request_duration_seconds`, `pichost_uploads_total`, etc.

- [ ] **Step 3: Update spec TODO**

```markdown
- [x] /metrics Prometheus 端点 (prometheus crate, HTTP counters/latency histograms, business gauges: uploads/users/images/storage)
```

- [ ] **Step 4: Update summary**

Add completion entry. Remaining: "OAuth 登录, CDN 集成, 水平扩展".

- [ ] **Step 5: Bump version**

```toml
version = "0.12.0"
```

- [ ] **Step 6: Commit**

```bash
git add docs/... .omo/... Cargo.toml Cargo.lock docs/superpowers/plans/...
git commit -m "chore: update spec and summary for prometheus metrics, bump version to 0.12.0"
```

---

## Self-Review Checklist

### 1. Spec Coverage
- ✅ Prometheus crate + registry: Task 1
- ✅ HTTP metrics (requests, latency): Task 2
- ✅ /metrics route: Task 2
- ✅ Business metrics (uploads, users, images, storage): Task 3
- ✅ Smoke test: Task 4

### 2. Placeholder Scan
- ✅ No "TBD", "TODO"
- ✅ All crate versions specified (prometheus 0.13, lazy_static 1.5)
- ✅ Histogram buckets defined explicitly

### 3. Type Consistency
- ✅ `CounterVec` labels `["method", "path", "status"]` consistent
- ✅ `HistogramVec` labels `["method", "path"]` consistent
- ✅ `IntGauge` for business metrics (total counts are integers)
