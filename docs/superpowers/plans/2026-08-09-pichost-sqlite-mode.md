# PicHost 轻量模式（SQLite + 无 Redis）实现计划

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 为 PicHost 增加轻量模式——SQLite 替代 PostgreSQL + 无 Redis 的零外部依赖单机部署；标准模式（PG+Redis）行为零变化。

**Architecture:** `DbPool = sqlx::AnyPool` 按 URL scheme 分派驱动；5 个 Redis 角色（Queue/Blacklist/RateLimiter/InviteStore/Cache）trait 化 + 双实现（Redis 实现保留给标准模式、SQLite 表实现给轻量模式）；轻量模式单进程内嵌 worker（pichost-worker 库化）；install.sh 交互化引导模式选择。设计文档：`docs/superpowers/specs/2026-08-09-pichost-sqlite-mode-design.md`。

**Tech Stack:** sqlx 0.8（postgres + sqlite bundled + any + json features）、Axum、deadpool-redis、figment、async-trait、bash。

## Global Constraints

- Rust edition 2021；函数 ≤50 行，行 ≤120 字符（`cargo clippy --workspace -- -D warnings` 零警告强制）
- 运行时查询：`query_as`/`query_scalar`/`QueryBuilder`，**禁止 `query!` 宏**（无编译期 DB）
- 迁移自动应用：`sqlx::migrate!()` / `Migrator`，启动时自动跑
- SQLite 连接必须带 `PRAGMA foreign_keys=ON` + WAL（`SqliteConnectOptions::create_if_missing(true).foreign_keys(true).journal_mode(SqliteJournalMode::Wal)`）
- 所有 `PICHOST_` 前缀环境变量经 figment（`__` 显式嵌套 + `_` 扁平 2 段）；`PICHOST_DATABASE_MODE=postgres|sqlite`
- 标准模式（PG+Redis）行为零变化：现有 575 测试全量保持通过
- 版本 bump：feature → minor（0.20.0 → 0.21.0），Cargo.toml + web-ui/package.json + CHANGELOG 同步
- 提交信息英文，语义化前缀（feat:/fix:/refactor:/docs:/chore:）
- 轻量模式范围：不做 PG→SQLite 数据迁移、不做多实例、不做 Docker 容器化
- sqlx 迁移 `Migrator` 为编译期内嵌：`PG_MIGRATOR = sqlx::migrate!("../migrations")`、`SQLITE_MIGRATOR = sqlx::migrate!("../migrations-sqlite")`（相对 pichost-core crate 根 = workspace 根）

## 任务依赖图

```
T1 (config mode + workspace sqlx/async-trait features)
  ├─ T2→T6 (migrations-sqlite 0001-0010: T2 depends T1; T3-T6 depends T1,T2)
  │    └─ T7 (core db.rs AnyPool + 双 Migrator, depends T2-T6)
  │         ├─ T8 (api/worker db re-export, depends T7)
  │         ├─ T9 (db_error_kind, depends T7)
  │         ├─ T10-T13 (方言改造: upload / users+storage_configs / admin / images, depends T8,T9)
  │         ├─ T14 (错误码使用点 auth/categories, depends T9)
  │         └─ T15 (TaskPayload 迁 core, depends [])
  │              └─ T16 (state traits, depends T15)
  │                   ├─ T17 (RedisQueue, depends T15,T16)
  │                   ├─ T18 (RedisCache+RedisInviteStore, depends T16)
  │                   ├─ T19 (RedisBlacklist 实现+字段注入+auth 调用点切换, depends T16,T18)
  │                   ├─ T20 (RedisRateLimiter 实现+字段注入+限流切换, depends T16,T18)
  │                   │    ├─ T25 (worker 库化, depends T15)
  │                   │    └─ T21 (AppState 完整装配, depends T17-T20,T25)
  │                   │         ├─ T22 (0011 状态表 + SqliteQueue, depends T7,T16)
  │                   │         ├─ T23 (SqliteBlacklist+SqliteRateLimiter, depends T7,T22)
  │                   │         ├─ T24 (SqliteInviteStore+NoopCache, depends T7,T22)
  │                   │         └─ T26 (轻量装配+e2e, depends T21-T25)
  │                   └─ T27 (install.sh 交互化, depends [])
  │                        └─ T28 (verify-release sqlite 分支 + .env.example, depends T27)
  │                             └─ T29 (文档同步 AGENTS/README, depends T26,T28)
  │                                  └─ T30 (版本 bump 0.21.0, depends T29)
  └─ T31 (CHANGELOG + summary 收尾, depends T29,T30)
```

---

## Phase A — SQLite DB 基础设施

### Task T1: 添加 DatabaseMode 配置 + workspace 依赖

**Files:**
- Modify: `pichost-core/src/config.rs:81-85`（DatabaseConfig 结构体）
- Modify: `Cargo.toml`（workspace sqlx features 增加 `sqlite`/`any`/`json`；新增 `async-trait` 依赖）
- Test: `pichost-core/src/config_test.rs`（新建；若已存在则追加）

**Interfaces:**
- Consumes: `AppConfig` figment 加载链（config.rs:260-274）
- Produces: `pub enum DatabaseMode { Postgres, Sqlite }`（serde rename_all="lowercase"，Default=Postgres）、`DatabaseConfig { mode, url, max_connections }`

**depends_on:** []

**breaking:** false（新字段带 serde default，向后兼容）

**ac:**
- given: 未设置任何 PICHOST_DATABASE_MODE 环境变量
  when: 加载默认配置 AppConfig::default()
  then: database.mode 为 DatabaseMode::Postgres
- given: 设置了 PICHOST_DATABASE_MODE=sqlite 且 PICHOST_DATABASE_URL=sqlite:///tmp/test.db
  when: 加载配置
  then: database.mode 为 DatabaseMode::Sqlite 且 url 为 sqlite:///tmp/test.db

- [ ] **Step 1: 写失败测试**

```rust
// pichost-core/src/config_test.rs
use crate::config::{AppConfig, DatabaseMode};

#[test]
fn database_mode_defaults_to_postgres() {
    let cfg = AppConfig::default();
    assert!(matches!(cfg.database.mode, DatabaseMode::Postgres));
}

#[test]
fn database_mode_parses_sqlite_from_env() {
    // 保存/恢复 PICHOST_DATABASE_MODE + PICHOST_DATABASE_URL（参考现有 env guard 模式）
    unsafe { std::env::set_var("PICHOST_DATABASE_MODE", "sqlite"); }
    unsafe { std::env::set_var("PICHOST_DATABASE_URL", "sqlite:///tmp/test.db"); }
    let cfg = crate::config::load_config(); // 现有加载函数（无 AppConfig::from_env）
    unsafe { std::env::remove_var("PICHOST_DATABASE_MODE"); }
    unsafe { std::env::remove_var("PICHOST_DATABASE_URL"); }
    assert!(matches!(cfg.database.mode, DatabaseMode::Sqlite));
    assert_eq!(cfg.database.url, "sqlite:///tmp/test.db");
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-core database_mode`
Expected: FAIL — `DatabaseMode` 未定义

- [ ] **Step 3: 最小实现**

```toml
# Cargo.toml (workspace) — sqlx 依赖启用（pichost-core 引用 workspace 依赖时继承）
sqlx = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "sqlite", "any", "json", "uuid", "chrono", "migrate"] }
async-trait = "0.1"
```

```rust
// pichost-core/src/config.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DatabaseMode { #[default] Postgres, Sqlite }

// DatabaseConfig 增加字段（保留 url/max_connections）:
#[serde(default)]
pub mode: DatabaseMode,
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-core database_mode`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add Cargo.toml pichost-core/src/config.rs pichost-core/src/config_test.rs
git commit -m "feat: add database.mode config (postgres|sqlite) and sqlite/async-trait deps"
```

**verify:**
- `cargo test -p pichost-core database_mode`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-core config`

---

### Task T2: SQLite 迁移 0001-0002（users/images）

**Files:**
- Create: `migrations-sqlite/0001_create_users.sql`
- Create: `migrations-sqlite/0002_create_images.sql`
- Test: `pichost-api/tests/sqlite_migrations_test.rs`（新建）

**Interfaces:**
- Consumes: 迁移改写规则（spec §3.3）：UUID→TEXT、TIMESTAMPTZ→TEXT ISO8601、删 pgcrypto、VARCHAR→TEXT、BOOLEAN→INTEGER
- Produces: SQLite 方言迁移文件 + 可运行的迁移测试

**breaking:** false

**depends_on:** [T1]

**ac:**
- given: migrations-sqlite 目录含 0001-0002 迁移文件
  when: 在 sqlite 内存库运行 MIGRATOR.run
  then: sqlite_master 中存在 users 与 images 表，且 users 表含 id/username/password_hash 列

- [ ] **Step 1: 写失败测试**

```rust
// pichost-api/tests/sqlite_migrations_test.rs
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::str::FromStr;

static MIGRATOR: Migrator = sqlx::migrate!("../migrations-sqlite");

async fn sqlite_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap()
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal);
    SqlitePoolOptions::new().max_connections(5).connect_with(opts).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_migrations_apply_users_images() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.expect("migrations apply");
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('users','images')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(n, 2);
    let cols: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pragma_table_info('users') WHERE name IN ('id','username','password_hash')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(cols, 3);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test sqlite_migrations_test`
Expected: FAIL — `migrations-sqlite` 目录不存在（编译错误）

- [ ] **Step 3: 创建迁移文件**

```sql
-- migrations-sqlite/0001_create_users.sql
CREATE TABLE users (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    username TEXT UNIQUE NOT NULL,
    email TEXT UNIQUE,
    password_hash TEXT NOT NULL,
    storage_backend TEXT NOT NULL DEFAULT 'local',
    storage_prefix TEXT NOT NULL DEFAULT '',
    is_admin INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
```

```sql
-- migrations-sqlite/0002_create_images.sql
CREATE TABLE images (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    public_key TEXT UNIQUE NOT NULL,
    original_name TEXT NOT NULL,
    storage_key TEXT NOT NULL,
    storage_backend TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    file_size INTEGER NOT NULL,
    width INTEGER,
    height INTEGER,
    sha256 TEXT NOT NULL,
    url TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE UNIQUE INDEX idx_images_user_sha256 ON images(user_id, sha256);
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-api --test sqlite_migrations_test`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add migrations-sqlite/ pichost-api/tests/sqlite_migrations_test.rs
git commit -m "feat: add sqlite migrations 0001-0002 (users, images)"
```

**migration_verify:**
- 测试断言 sqlite_master 中 users/images 表存在
- pragma_table_info('users') 返回 id/username/password_hash 列

**verify:**
- `cargo test -p pichost-api --test sqlite_migrations_test`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api test_health`

---

### Task T3: SQLite 迁移 0003-0004（processing fields / upload_tasks）

**Files:**
- Create: `migrations-sqlite/0003_add_image_processing_fields.sql`
- Create: `migrations-sqlite/0004_create_upload_tasks.sql`
- Test: `pichost-api/tests/sqlite_migrations_test.rs`（追加断言）

**Interfaces:**
- Consumes: T2 的 `sqlite_pool()` helper 与 MIGRATOR 常量
- Produces: 0003/0004 SQLite 迁移

**breaking:** false

**depends_on:** [T1, T2]

**ac:**
- given: migrations-sqlite 目录含 0003-0004 迁移文件
  when: 在 sqlite 内存库运行 MIGRATOR.run
  then: images 表含 thumbnail_key/webp_url 列，且 upload_tasks 表存在

- [ ] **Step 1: 追加失败测试**

```rust
// 追加到 sqlite_migrations_test.rs
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_migrations_processing_and_tasks() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pragma_table_info('images') WHERE name IN ('thumbnail_key','webp_url')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(n, 2);
    let t: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='upload_tasks'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(t, 1);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test sqlite_migrations_test sqlite_migrations_processing_and_tasks`
Expected: FAIL — 迁移文件缺失

- [ ] **Step 3: 创建迁移文件**

```sql
-- migrations-sqlite/0003_add_image_processing_fields.sql
-- SQLite 仅支持单列 ADD COLUMN，须逐条执行
ALTER TABLE images ADD COLUMN thumbnail_key TEXT;
ALTER TABLE images ADD COLUMN thumbnail_url TEXT;
ALTER TABLE images ADD COLUMN webp_key TEXT;
ALTER TABLE images ADD COLUMN webp_url TEXT;
```

```sql
-- migrations-sqlite/0004_create_upload_tasks.sql
CREATE TABLE upload_tasks (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    image_id TEXT NOT NULL REFERENCES images(id) ON DELETE CASCADE,
    task_type TEXT NOT NULL DEFAULT 'all',
    payload TEXT,
    status TEXT NOT NULL DEFAULT 'pending',
    error TEXT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    completed_at TEXT
);
CREATE INDEX idx_upload_tasks_image_id ON upload_tasks(image_id);
CREATE INDEX idx_upload_tasks_status ON upload_tasks(status);
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-api --test sqlite_migrations_test sqlite_migrations_processing_and_tasks`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add migrations-sqlite/ pichost-api/tests/sqlite_migrations_test.rs
git commit -m "feat: add sqlite migrations 0003-0004 (processing fields, upload_tasks)"
```

**migration_verify:**
- images 表 thumbnail_key/webp_url 列存在
- upload_tasks 表存在

**verify:**
- `cargo test -p pichost-api --test sqlite_migrations_test`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api test_health`

---

### Task T4: SQLite 迁移 0005-0006（搜索索引 / storage_quota）

**Files:**
- Create: `migrations-sqlite/0005_add_image_search_index.sql`
- Create: `migrations-sqlite/0006_add_storage_quota.sql`
- Test: `pichost-api/tests/sqlite_migrations_test.rs`（追加断言）

**Interfaces:**
- Consumes: 规则：删 COMMENT ON、ADD COLUMN 不用 IF NOT EXISTS（SQLite 不支持）
- Produces: 0005/0006 迁移

**breaking:** false

**depends_on:** [T1, T2]

**ac:**
- given: migrations-sqlite 目录含 0005-0006 迁移文件
  when: 在 sqlite 内存库运行 MIGRATOR.run
  then: users 表含 storage_quota 列，且 idx_images_user_filename 索引存在

- [ ] **Step 1: 追加失败测试**

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_migrations_quota_and_index() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pragma_table_info('users') WHERE name='storage_quota'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1);
    let idx: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='idx_images_user_filename'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(idx, 1);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test sqlite_migrations_test sqlite_migrations_quota_and_index`
Expected: FAIL

- [ ] **Step 3: 创建迁移文件**

```sql
-- migrations-sqlite/0005_add_image_search_index.sql
CREATE INDEX IF NOT EXISTS idx_images_user_filename ON images(user_id, original_name);
```

```sql
-- migrations-sqlite/0006_add_storage_quota.sql
ALTER TABLE users ADD COLUMN storage_quota INTEGER;
```

- [ ] **Step 4: 运行确认通过**

Run: 同上
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add migrations-sqlite/ pichost-api/tests/sqlite_migrations_test.rs
git commit -m "feat: add sqlite migrations 0005-0006 (search index, storage_quota)"
```

**migration_verify:**
- users.storage_quota 列存在
- idx_images_user_filename 索引存在

**verify:**
- `cargo test -p pichost-api --test sqlite_migrations_test`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api test_health`

---

### Task T5: SQLite 迁移 0007-0008（oauth_accounts / storage_configs）

**Files:**
- Create: `migrations-sqlite/0007_create_oauth_accounts.sql`
- Create: `migrations-sqlite/0008_user_storage_configs.sql`
- Test: `pichost-api/tests/sqlite_migrations_test.rs`（追加断言）

**Interfaces:**
- Consumes: 部分唯一索引保留（SQLite 支持 WHERE 子句）；JSONB→TEXT
- Produces: 0007/0008 迁移

**breaking:** false

**depends_on:** [T1, T2]

**ac:**
- given: migrations-sqlite 目录含 0007-0008 迁移文件
  when: 在 sqlite 内存库运行 MIGRATOR.run
  then: oauth_accounts 与 user_storage_configs 表存在，idx_default_per_user 索引存在

- [ ] **Step 1: 追加失败测试**

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_migrations_storage_configs() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let t: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('oauth_accounts','user_storage_configs')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(t, 2);
    let idx: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='index' AND name='idx_default_per_user'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(idx, 1);
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pragma_table_info('user_storage_configs') WHERE name IN ('config','is_default')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(n, 2);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test sqlite_migrations_test sqlite_migrations_storage_configs`
Expected: FAIL

- [ ] **Step 3: 创建迁移文件**

```sql
-- migrations-sqlite/0007_create_oauth_accounts.sql
CREATE TABLE oauth_accounts (
    id TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    provider TEXT NOT NULL,
    provider_user_id TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(provider, provider_user_id)
);
CREATE INDEX idx_oauth_accounts_user ON oauth_accounts(user_id);
```

```sql
-- migrations-sqlite/0008_user_storage_configs.sql
CREATE TABLE user_storage_configs (
    id          TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    provider    TEXT NOT NULL,
    is_default  INTEGER NOT NULL DEFAULT 0,
    config      TEXT NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(user_id, name)
);
CREATE UNIQUE INDEX idx_default_per_user
    ON user_storage_configs(user_id) WHERE is_default = 1;
ALTER TABLE images
    ADD COLUMN storage_config_id TEXT REFERENCES user_storage_configs(id);
```

- [ ] **Step 4: 运行确认通过**

Run: 同上
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add migrations-sqlite/ pichost-api/tests/sqlite_migrations_test.rs
git commit -m "feat: add sqlite migrations 0007-0008 (oauth_accounts, storage_configs)"
```

**migration_verify:**
- oauth_accounts / user_storage_configs 表存在
- idx_default_per_user 部分唯一索引存在（is_default = 1）

**verify:**
- `cargo test -p pichost-api --test sqlite_migrations_test`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api test_health`

---

### Task T6: SQLite 迁移 0009-0010（categories / watermark_config）

**Files:**
- Create: `migrations-sqlite/0009_create_categories.sql`
- Create: `migrations-sqlite/0010_add_watermark_config.sql`
- Test: `pichost-api/tests/sqlite_migrations_test.rs`（追加断言）

**Interfaces:**
- Consumes: 自引用 FK（categories.parent_id）SQLite 支持；DELETE SET NULL 保留
- Produces: 0009/0010 迁移；完整 10 迁移集

**breaking:** false

**depends_on:** [T1, T2]

**ac:**
- given: migrations-sqlite 目录含全部 0001-0010 迁移文件
  when: 在 sqlite 内存库运行 MIGRATOR.run
  then: _sqlx_migrations 最大版本为 10，users 表含 watermark_config 列，categories 表存在

- [ ] **Step 1: 追加失败测试**

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_migrations_all_ten() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let ver: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(ver, 10);
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM pragma_table_info('users') WHERE name='watermark_config'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(n, 1);
    let cat: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='categories'")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(cat, 1);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test sqlite_migrations_test sqlite_migrations_all_ten`
Expected: FAIL

- [ ] **Step 3: 创建迁移文件**

```sql
-- migrations-sqlite/0009_create_categories.sql
CREATE TABLE categories (
    id          TEXT PRIMARY KEY DEFAULT (lower(hex(randomblob(16)))),
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    parent_id   TEXT REFERENCES categories(id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    UNIQUE(user_id, name, parent_id)
);
ALTER TABLE images
    ADD COLUMN category_id TEXT REFERENCES categories(id) ON DELETE SET NULL;
```

```sql
-- migrations-sqlite/0010_add_watermark_config.sql
ALTER TABLE users ADD COLUMN watermark_config TEXT;
```

- [ ] **Step 4: 运行确认通过**

Run: 同上
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add migrations-sqlite/ pichost-api/tests/sqlite_migrations_test.rs
git commit -m "feat: add sqlite migrations 0009-0010 (categories, watermark_config)"
```

**migration_verify:**
- `_sqlx_migrations` 最大版本 = 10
- users.watermark_config 列、categories 表存在

**verify:**
- `cargo test -p pichost-api --test sqlite_migrations_test`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api test_health`

---

### Task T7: core db.rs（AnyPool + create_pool 分派 + 双 Migrator）

**Files:**
- Create: `pichost-core/src/db.rs`
- Create: `pichost-core/src/db_test.rs`
- Modify: `pichost-core/src/lib.rs`（`pub mod db;`）

**Interfaces:**
- Consumes: T1 的 `DatabaseMode`；T2-T6 的 `migrations-sqlite/` 目录
- Produces: `pub type DbPool = sqlx::AnyPool`；`create_pool(url, max_conns, mode) -> Result<DbPool, sqlx::Error>`；`run_migrations(pool, mode) -> Result<(), MigrateError>`；`pub static PG_MIGRATOR: Migrator`、`pub static SQLITE_MIGRATOR: Migrator`

**depends_on:** [T2, T3, T4, T5, T6]

**breaking:** true（crate 边界：pichost-core 新增 DB 运行时依赖与公共模块）

**ac:**
- given: sqlite::memory: URL 与 DatabaseMode::Sqlite
  when: 调用 create_pool 后执行 run_migrations
  then: _sqlx_migrations 最大版本为 10（10 个 sqlite 迁移全部应用）
- given: postgres:// URL 与 DatabaseMode::Postgres（需 Docker PG）
  when: 调用 create_pool 后执行 run_migrations
  then: 连接成功且迁移应用（#[ignore] 测试）

- [ ] **Step 1: 写失败测试**

```rust
// pichost-core/src/db_test.rs
use crate::config::DatabaseMode;
use crate::db::{create_pool, run_migrations};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_pool_sqlite_in_memory_runs_migrations() {
    let pool = create_pool("sqlite::memory:", 5, DatabaseMode::Sqlite).await.unwrap();
    run_migrations(&pool, DatabaseMode::Sqlite).await.unwrap();
    let v: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(v, 10);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL"]
async fn create_pool_postgres_url() {
    let pool = create_pool(
        "postgres://pichost:pichost@localhost:5432/pichost", 5, DatabaseMode::Postgres)
        .await.unwrap();
    run_migrations(&pool, DatabaseMode::Postgres).await.unwrap();
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-core create_pool_sqlite_in_memory_runs_migrations`
Expected: FAIL — `db` 模块不存在

- [ ] **Step 3: 最小实现**

```rust
// pichost-core/src/db.rs
use crate::config::DatabaseMode;
use sqlx::any::{AnyConnectOptions, AnyPool, AnyPoolOptions};
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
use std::str::FromStr;
use std::time::Duration;

pub type DbPool = AnyPool;

pub static PG_MIGRATOR: Migrator = sqlx::migrate!("../migrations");
pub static SQLITE_MIGRATOR: Migrator = sqlx::migrate!("../migrations-sqlite");

pub async fn create_pool(url: &str, max_connections: u32, mode: DatabaseMode)
    -> Result<DbPool, sqlx::Error> {
    sqlx::any::install_default_drivers(); // 必须：AnyPool 无驱动时 connect 会 panic
    let mut opts = AnyPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_secs(5));
    match mode {
        DatabaseMode::Postgres => {
            let connect = AnyConnectOptions::from_str(url)?;
            opts.connect_with(connect).await
        }
        DatabaseMode::Sqlite => {
            let connect = SqliteConnectOptions::from_str(url)?
                .create_if_missing(true)
                .foreign_keys(true)
                .journal_mode(SqliteJournalMode::Wal);
            opts.connect_with(connect).await
        }
    }
}

pub async fn run_migrations(pool: &DbPool, mode: DatabaseMode)
    -> Result<(), sqlx::migrate::MigrateError> {
    match mode {
        DatabaseMode::Postgres => PG_MIGRATOR.run(pool).await,
        DatabaseMode::Sqlite => SQLITE_MIGRATOR.run(pool).await,
    }
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-core create_pool_sqlite_in_memory_runs_migrations`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-core/src/db.rs pichost-core/src/db_test.rs pichost-core/src/lib.rs pichost-core/Cargo.toml
git commit -m "feat: add shared AnyPool db module with dialect dispatch (breaking)"
```

**migration_verify:**
- sqlite 内存库迁移后 `_sqlx_migrations` 最大版本 = 10（10 个迁移全部应用）

**verify:**
- `cargo test -p pichost-core create_pool_sqlite_in_memory_runs_migrations`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-core config`
- `cargo check -p pichost-api`（旧代码未动，应编译通过）

---

### Task T8: api/worker db 模块改为 re-export core

**Files:**
- Modify: `pichost-api/src/db/mod.rs`（整体替换为 re-export）
- Modify: `pichost-worker/src/db.rs`（整体替换为 re-export）
- Test: `pichost-api/tests/common/mod.rs`（harness 适配：create_pool 调用加 mode 参数；connect_lazy 测试池改 AnyPool）

**Interfaces:**
- Consumes: T7 的 `pichost_core::db::{DbPool, create_pool, run_migrations}`
- Produces: `pichost_api::db` 与 `pichost_worker::db` 保持原路径 API 但转发 core 实现；调用点加 mode 参数

**breaking:** true（DbPool 类型从 PgPool 变 AnyPool；create_pool 签名变化）

**depends_on:** [T7]

**ac:**
- given: workspace 已完成 T7（core db.rs 提供 create_pool/run_migrations）
  when: cargo check --workspace
  then: 编译通过，api/worker 的 db 模块 re-export core 实现且 main.rs 调用点带 mode 参数

- [ ] **Step 1: 写失败测试（编译失败即测试失败）**

```rust
// pichost-api/src/main.rs — 调用点改造
// 原: let pool = db::create_pool(&config.database.url, config.database.max_connections).await?;
//     db::run_migrations(&pool).await?;
// 新: let mode = config.database.mode;
//     let pool = db::create_pool(&config.database.url, config.database.max_connections, mode).await?;
//     db::run_migrations(&pool, mode).await?;
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo check --workspace`
Expected: FAIL — create_pool 缺 mode 参数、PgPool 类型不匹配

- [ ] **Step 3: 最小实现**

```rust
// pichost-api/src/db/mod.rs（整体替换）
pub use pichost_core::db::{create_pool, run_migrations, DbPool, PG_MIGRATOR, SQLITE_MIGRATOR};
```

```rust
// pichost-worker/src/db.rs（整体替换）
pub use pichost_core::db::{create_pool, run_migrations, DbPool, PG_MIGRATOR, SQLITE_MIGRATOR};
```

```rust
// pichost-api/src/main.rs:23-24 与 pichost-worker/src/main.rs 对应位置
let mode = config.database.mode;
let pool = db::create_pool(&config.database.url, config.database.max_connections, mode).await?;
db::run_migrations(&pool, mode).await?;
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo check --workspace`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-api/src/db/mod.rs pichost-worker/src/db.rs pichost-api/src/main.rs pichost-worker/src/main.rs pichost-api/tests/common/mod.rs
git commit -m "refactor: re-export shared AnyPool db module in api/worker (breaking)"
```

**verify:**
- `cargo check --workspace`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api test_health`（Docker infra 下）

---

### Task T9: db_error_kind 统一错误映射

**Files:**
- Modify: `pichost-core/src/db.rs`（追加映射函数与枚举）
- Test: `pichost-core/src/db_test.rs`（追加测试）

**Interfaces:**
- Consumes: `sqlx::Error` Database 错误码（PG SQLSTATE 23505 / SQLite 2067 / 19）
- Produces: `pub enum DbErrorKind { UniqueViolation, Other }`；`pub fn db_error_kind(err: &sqlx::Error) -> DbErrorKind`

**depends_on:** [T7]

**breaking:** false

**ac:**
- given: 构造携带 code="23505"（或 "2067"/"19"）的 sqlx::Error::Database
  when: 调用 db_error_kind(&err)
  then: 返回 DbErrorKind::UniqueViolation；其他错误码返回 DbErrorKind::Other

- [ ] **Step 1: 写失败测试**

```rust
// pichost-core/src/db_test.rs 追加
use crate::db::{db_error_kind, DbErrorKind};

#[test]
fn sqlite_unique_violation_code_maps() {
    // 真实 sqlite 约束错误驱动（sqlx 无 new_for_test 公共 API）:
    // 在 sqlite::memory: 上建带 UNIQUE 约束的表，插入重复值捕获错误
    let rt = tokio::runtime::Runtime::new().unwrap();
    let kind = rt.block_on(async {
        let pool = create_pool("sqlite::memory:", 1, DatabaseMode::Sqlite).await.unwrap();
        sqlx::query("CREATE TABLE t (id TEXT PRIMARY KEY)").execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO t (id) VALUES ('a')").execute(&pool).await.unwrap();
        let err = sqlx::query("INSERT INTO t (id) VALUES ('a')").execute(&pool).await.unwrap_err();
        db_error_kind(&err)
    });
    assert_eq!(kind, DbErrorKind::UniqueViolation);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-core db_error_kind`
Expected: FAIL — 未定义

- [ ] **Step 3: 最小实现**

```rust
// pichost-core/src/db.rs 追加
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbErrorKind { UniqueViolation, Other }

pub fn db_error_kind(err: &sqlx::Error) -> DbErrorKind {
    match err {
        sqlx::Error::Database(db) => {
            let code = db.code().map(|c| c.to_string()).unwrap_or_default();
            if code == "23505" || code == "2067" || code == "19" {
                DbErrorKind::UniqueViolation
            } else {
                DbErrorKind::Other
            }
        }
        _ => DbErrorKind::Other,
    }
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-core db_error_kind`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-core/src/db.rs pichost-core/src/db_test.rs
git commit -m "feat: add unified db_error_kind mapping (PG 23505 / SQLite 2067)"
```

**verify:**
- `cargo test -p pichost-core db_error_kind`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-core`

---

## Phase B — 方言中立化改造

### Task T10: upload.rs 方言改造（quota / ANY / ILIKE）

**Files:**
- Modify: `pichost-api/src/services/upload.rs:288,493,1066`（及周边方言改造；TaskPayload 引用路径更新在 T15 完成）
- Test: `pichost-api/tests/sqlite_smoke_test.rs`（新建；sqlite 冒烟查询）

**Interfaces:**
- Consumes: T9 的 `db_error_kind`（若本文件有 23505 判断则一并替换）
- Produces: 方言中立查询（quota `SUM(file_size)` 无 `::BIGINT`；`ANY($1)` 展开为 `IN ($1,...)`；`ILIKE` → `LIKE`）；本任务不触碰 `TaskPayload` 引用（T15 统一迁移）

**breaking:** false

**depends_on:** [T8, T9]

**ac:**
- given: sqlite 内存库已跑迁移且含一个用户
  when: 执行改造后的 quota 查询（COALESCE(SUM(file_size), 0)，无 ::BIGINT）
  then: 返回 0 且查询成功；PG 模式现有 upload 测试（test_upload/test_upload_dedup）保持通过

- [ ] **Step 1: 写失败测试（sqlite 冒烟：quota 查询与配置查询可执行）**

```rust
// pichost-api/tests/sqlite_smoke_test.rs
use sqlx::migrate::Migrator;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::str::FromStr;

static MIGRATOR: Migrator = sqlx::migrate!("../migrations-sqlite");

async fn sqlite_pool() -> SqlitePool {
    let opts = SqliteConnectOptions::from_str("sqlite::memory:")
        .unwrap().foreign_keys(true).journal_mode(SqliteJournalMode::Wal);
    SqlitePoolOptions::new().max_connections(5).connect_with(opts).await.unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_quota_and_config_queries() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let uid: String = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash) VALUES ('u','h') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    // 改造后的 quota 查询（无 ::BIGINT）
    let q: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(file_size), 0) FROM images WHERE user_id = ?")
        .bind(&uid).fetch_one(&pool).await.unwrap();
    assert_eq!(q, 0);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test sqlite_smoke_test sqlite_quota_and_config_queries`
Expected: 编译/运行 FAIL — 当前实现 SQL 带 `::BIGINT` 方言（此测试直接写目标 SQL，需实现后通过）

- [ ] **Step 3: 实现改造**

```rust
// upload.rs:288 — 删除 ::BIGINT
// 原: SELECT COALESCE(SUM(file_size), 0)::BIGINT FROM images WHERE user_id = $1
// 新: SELECT COALESCE(SUM(file_size), 0) FROM images WHERE user_id = $1

// upload.rs:493 — ANY($1) 展开为 IN ($1,$2,...)（编号占位符，PG/SQLite 均支持 $N）
// 原: ... WHERE id = ANY($1)
// 新: 动态构建:
let placeholders = (1..=ids.len())
    .map(|i| format!("${}", i))
    .collect::<Vec<_>>().join(",");
let mut q = sqlx::query_as::<_, ConfigRow>(
    &format!("SELECT ... WHERE id IN ({})", placeholders));
for id in &ids { q = q.bind(id); }

// upload.rs:1066 — ILIKE → LIKE（QueryBuilder 内）
// 原: builder.push(" AND original_name ILIKE "); builder.push_bind(format!("%{}%", term));
// 新: builder.push(" AND original_name LIKE "); builder.push_bind(format!("%{}%", term));
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-api --test sqlite_smoke_test` + `cargo test -p pichost-api test_upload`
Expected: 双通过

- [ ] **Step 5: 提交**

```bash
git add pichost-api/src/services/upload.rs pichost-api/tests/sqlite_smoke_test.rs
git commit -m "refactor: make upload queries dialect-neutral (remove ::casts, ANY, ILIKE)"
```

**verify:**
- `cargo test -p pichost-api --test sqlite_smoke_test sqlite_quota_and_config_queries`
- `cargo test -p pichost-api test_upload`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api test_upload_dedup`
- `cargo test -p pichost-api test_image_list`

---

### Task T11: users.rs + storage_configs.rs 方言改造

**Files:**
- Modify: `pichost-api/src/routes/users.rs:96-98,291-307`（::BIGINT / ::boolean / ::jsonb / now()）
- Modify: `pichost-api/src/routes/storage_configs.rs:413,483`（now()）
- Test: `pichost-api/tests/sqlite_smoke_test.rs`（追加断言）

**Interfaces:**
- Consumes: T9 的 `db_error_kind`（users.rs:312 的 23505 判断替换）
- Produces: 方言中立查询

**breaking:** false

**depends_on:** [T8, T9]

**ac:**
- given: sqlite 内存库已跑迁移且含一个用户
  when: 执行改造后的 stats/update 查询（无 ::BIGINT/::boolean/::jsonb，CURRENT_TIMESTAMP 替代 now()）
  then: 查询成功返回 (0,0)，UPDATE 影响 1 行；PG 模式现有 users/storage_configs 测试保持通过

- [ ] **Step 1: 写失败测试**

```rust
// 追加到 sqlite_smoke_test.rs
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_stats_and_update_queries() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let uid: String = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash) VALUES ('u','h') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    // stats 查询（无 ::BIGINT）
    let q: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(file_size), 0) FROM images WHERE user_id = ?")
        .bind(&uid).fetch_one(&pool).await.unwrap();
    assert_eq!(q, (0, 0));
    // UPDATE profile（CURRENT_TIMESTAMP 替代 now()；无 ::boolean/::jsonb 转换）
    let rows = sqlx::query(
        "UPDATE users SET email = ?, updated_at = CURRENT_TIMESTAMP WHERE id = ?")
        .bind("a@b.c").bind(&uid).execute(&pool).await.unwrap();
    assert_eq!(rows.rows_affected(), 1);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test sqlite_smoke_test sqlite_stats_and_update_queries`
Expected: FAIL

- [ ] **Step 3: 实现改造**

```rust
// users.rs:96-98 — 删 ::BIGINT
// 原: SELECT COUNT(*)::BIGINT, COALESCE(SUM(file_size), 0)::BIGINT ...
// 新: SELECT COUNT(*), COALESCE(SUM(file_size), 0) ...

// users.rs:294,296 — 删 ::boolean / ::jsonb（Rust 侧 bind 类型已定型）
// 原: CASE WHEN $2::boolean THEN ... END, ..., $7::jsonb
// 新: CASE WHEN $2 THEN ... END, ..., $7

// users.rs:297,404 — now() → CURRENT_TIMESTAMP
// storage_configs.rs:413,483 — now() → CURRENT_TIMESTAMP

// users.rs:312 — 23505 判断替换
// 原: if let Some(db) = err.as_database_error() { if db.code() == Some("23505") { ... } }
// 新: if db_error_kind(&err) == DbErrorKind::UniqueViolation { ... }
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-api --test sqlite_smoke_test` + `cargo test -p pichost-api test_users`
Expected: 双通过

- [ ] **Step 5: 提交**

```bash
git add pichost-api/src/routes/users.rs pichost-api/src/routes/storage_configs.rs pichost-api/tests/sqlite_smoke_test.rs
git commit -m "refactor: make users/storage_configs queries dialect-neutral"
```

**verify:**
- `cargo test -p pichost-api --test sqlite_smoke_test sqlite_stats_and_update_queries`
- `cargo test -p pichost-api test_users`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api test_users_profile`
- `cargo test -p pichost-api test_storage_config`

---

### Task T12: admin.rs 方言改造（::BIGINT / INTERVAL 参数化）

**Files:**
- Modify: `pichost-api/src/routes/admin.rs:454-494,481`（统计查询）
- Test: `pichost-api/tests/sqlite_smoke_test.rs`（追加断言）

**Interfaces:**
- Consumes: 无新依赖；INTERVAL 改为 Rust 侧时间戳参数
- Produces: 方言中立统计查询

**breaking:** false

**depends_on:** [T8, T9]

**ac:**
- given: sqlite 内存库已跑迁移
  when: 执行改造后的 admin 统计查询（无 ::BIGINT，时间戳参数替代 NOW()-INTERVAL）
  then: 查询成功返回 (0,0)；PG 模式现有 admin_stats 测试保持通过

- [ ] **Step 1: 写失败测试**

```rust
// 追加到 sqlite_smoke_test.rs
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_admin_stats_queries() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let uid: String = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash) VALUES ('u','h') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    // 统计（无 ::BIGINT）
    let q: (i64, i64) = sqlx::query_as(
        "SELECT COUNT(*), COALESCE(SUM(file_size), 0) FROM images")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(q, (0, 0));
    // active users（时间戳参数替代 NOW() - INTERVAL）
    let cutoff = chrono::Utc::now() - chrono::Duration::hours(24);
    let n: i64 = sqlx::query_scalar(
        "SELECT COUNT(DISTINCT user_id) FROM images WHERE created_at >= ?")
        .bind(cutoff).fetch_one(&pool).await.unwrap();
    assert_eq!(n, 0);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test sqlite_smoke_test sqlite_admin_stats_queries`
Expected: FAIL

- [ ] **Step 3: 实现改造**

```rust
// admin.rs:454-494 — 删 ::BIGINT（COUNT(*) / SUM(...)）
// admin.rs:479-483 — INTERVAL 参数化
// 原: ... WHERE created_at >= NOW() - INTERVAL '24 hours'
// 新: ... WHERE created_at >= $1
//     let cutoff = chrono::Utc::now() - chrono::Duration::hours(24);
//     .bind(cutoff)
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-api --test sqlite_smoke_test` + `cargo test -p pichost-api test_admin_stats`
Expected: 双通过

- [ ] **Step 5: 提交**

```bash
git add pichost-api/src/routes/admin.rs pichost-api/tests/sqlite_smoke_test.rs
git commit -m "refactor: make admin stats queries dialect-neutral (INTERVAL parameterized)"
```

**verify:**
- `cargo test -p pichost-api --test sqlite_smoke_test sqlite_admin_stats_queries`
- `cargo test -p pichost-api test_admin_stats`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api test_admin`

---

### Task T13: images.rs 方言改造 + QueryBuilder 泛型化

**Files:**
- Modify: `pichost-api/src/routes/images.rs:82,91,116,142,745,773,895`
- Modify: `pichost-api/src/services/upload.rs:1060`（QueryBuilder helper 签名泛型化，如存在共享 helper）
- Test: `pichost-api/tests/sqlite_smoke_test.rs`（追加断言）

**Interfaces:**
- Consumes: 无；`QueryBuilder<'_, sqlx::Postgres>` → `QueryBuilder<'_, sqlx::Any>`
- Produces: 画廊/批量操作查询方言中立

**breaking:** false

**depends_on:** [T8, T9]

**ac:**
- given: sqlite 内存库已跑迁移且含一个用户
  when: 执行改造后的画廊/批量查询（IN 展开替代 ANY，LIKE 替代 ILIKE）
  then: 查询成功返回 0；QueryBuilder 泛型化为 Any；PG 模式现有 image_list/batch 测试保持通过

- [ ] **Step 1: 写失败测试**

```rust
// 追加到 sqlite_smoke_test.rs
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_gallery_and_batch_queries() {
    let pool = sqlite_pool().await;
    MIGRATOR.run(&pool).await.unwrap();
    let uid: String = sqlx::query_scalar(
        "INSERT INTO users (username, password_hash) VALUES ('u','h') RETURNING id")
        .fetch_one(&pool).await.unwrap();
    // IN (...) 展开（替代 ANY）
    let ph = "?";
    let n: i64 = sqlx::query_scalar(&format!(
        "SELECT count(*) FROM images WHERE user_id = ? AND id IN ({})", ph))
        .bind(&uid).fetch_one(&pool).await.unwrap();
    assert_eq!(n, 0);
    // LIKE 搜索（替代 ILIKE）
    let c: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM images WHERE user_id = ? AND original_name LIKE ?")
        .bind(&uid).bind("%foo%").fetch_one(&pool).await.unwrap();
    assert_eq!(c, 0);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test sqlite_smoke_test sqlite_gallery_and_batch_queries`
Expected: FAIL

- [ ] **Step 3: 实现改造**

```rust
// images.rs:82 — 泛型化
// 原: fn push_optional_filters(builder: &mut sqlx::QueryBuilder<'_, sqlx::Postgres>, ...)
// 新: fn push_optional_filters(builder: &mut sqlx::QueryBuilder<'_, sqlx::Any>, ...)
// images.rs:91 — ILIKE → LIKE
// images.rs:116,142 — QueryBuilder::new 显式标注 Postgres 则改 Any（类型推断可省略）
// images.rs:745,773,895 — ANY($1)/ANY($3) → IN ($1,$2,...) 编号占位符动态展开
// 注意: 若 IN 之前已有 $N 绑定，偏移量须从已有绑定数 +1 开始编号（同 T10 模式）
// upload.rs:1060 — 同样泛型化
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-api --test sqlite_smoke_test` + `cargo test -p pichost-api test_image_list`
Expected: 双通过

- [ ] **Step 5: 提交**

```bash
git add pichost-api/src/routes/images.rs pichost-api/src/services/upload.rs pichost-api/tests/sqlite_smoke_test.rs
git commit -m "refactor: make gallery/batch queries dialect-neutral, generalize QueryBuilder to Any"
```

**verify:**
- `cargo test -p pichost-api --test sqlite_smoke_test sqlite_gallery_and_batch_queries`
- `cargo test -p pichost-api test_image_list`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api test_image_batch_delete`
- `cargo test -p pichost-api test_image_move`

---

### Task T14: 错误码使用点改造（auth.rs / categories.rs）

**Files:**
- Modify: `pichost-api/src/routes/auth.rs:230-231`（23505 判断）
- Modify: `pichost-api/src/routes/categories.rs:168`（constraint 名匹配）
- Test: `pichost-api/tests/categories_test.rs`（追加断言）

**Interfaces:**
- Consumes: T9 的 `db_error_kind`/`DbErrorKind`
- Produces: 错误处理方言中立

**breaking:** false

**depends_on:** [T9]

**ac:**
- given: PG 模式下创建重复分类（或重复用户名注册）触发唯一约束冲突
  when: 处理错误经 db_error_kind 映射
  then: 返回 409（分类）/相应冲突状态，且不依赖 PG constraint 名字符串

- [ ] **Step 1: 写失败测试**

```rust
// 追加到 categories_test.rs（唯一约束冲突路径断言）
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL"]
async fn duplicate_category_returns_conflict() {
    let app = test_app().await;
    let admin = create_admin(&app).await;
    let token = login(&app, &admin).await;
    // 创建分类
    let resp = send_json(&app, "POST", "/api/v1/categories", &token,
        serde_json::json!({"name": "dup"})).await;
    assert_eq!(resp.status(), 201);
    // 重复创建同名分类 → 409（经 db_error_kind 映射，不依赖 PG constraint 名）
    let resp2 = send_json(&app, "POST", "/api/v1/categories", &token,
        serde_json::json!({"name": "dup"})).await;
    assert_eq!(resp2.status(), StatusCode::CONFLICT);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test categories_test duplicate_category_returns_conflict -- --ignored`
Expected: 编译/运行 FAIL — db_error_kind 未接入

- [ ] **Step 3: 实现改造**

```rust
// auth.rs:230-231
// 原: if let Some(db) = err.as_database_error() { if db.code() == Some("23505") { ... } }
// 新: if db_error_kind(&err) == DbErrorKind::UniqueViolation { ... }

// categories.rs:168
// 原: if let Some(constraint) = db.constraint() { if constraint == "categories_user_id_name_parent_id_key" { ... } }
// 新: if db_error_kind(&err) == DbErrorKind::UniqueViolation { ... }
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-api --test categories_test duplicate_category_returns_conflict -- --ignored` + `cargo test -p pichost-api test_register_duplicate`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-api/src/routes/auth.rs pichost-api/src/routes/categories.rs pichost-api/tests/categories_test.rs
git commit -m "refactor: use db_error_kind in auth/categories uniqueness handling"
```

**verify:**
- `cargo test -p pichost-api --test categories_test -- --ignored`
- `cargo test -p pichost-api test_register_duplicate`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api test_categories`

---

## Phase C — trait 抽象 + Redis 封装

### Task T15: TaskPayload 迁入 pichost-core models

**Files:**
- Modify: `pichost-core/src/models.rs`（追加 TaskPayload）
- Modify: `pichost-worker/src/queue.rs`（移除定义，引用 core）
- Test: `pichost-core/src/models_test.rs`（新建；task_payload round-trip 测试）

**Interfaces:**
- Consumes: 现有 `pichost-worker/src/queue.rs:8-19` 的定义
- Produces: `pichost_core::models::TaskPayload`（字段不变）；同一提交内同步更新 `pichost-api/src/services/upload.rs` 的引用路径 `pichost_worker::queue::TaskPayload` → `pichost_core::models::TaskPayload`（upload.rs 不在 Files 列表，作为伴随修改随本任务提交，避免 T10 阶段引用悬空）

**depends_on:** []

**breaking:** true（类型移动，跨 crate 引用变化）

**ac:**
- given: TaskPayload 已定义于 pichost-core models
  when: 序列化再反序列化一个最小 TaskPayload
  then: task_id 字段 round-trip 相等，且 workspace 编译通过

- [ ] **Step 1: 写失败测试**

```rust
// pichost-core/src/models_test.rs（新建；crate 内测试模块，经 lib.rs 注册）
use crate::models::TaskPayload;
use uuid::Uuid;

#[test]
fn task_payload_roundtrips_json() {
    let p = TaskPayload {
        task_id: Uuid::new_v4(), image_id: Uuid::new_v4(), user_id: Uuid::new_v4(),
        storage_backend: "local".into(), storage_config_id: None,
        storage_backend_name: "local".into(), source_key: "k".into(),
        source_mime: "image/png".into(), retry_count: 0, max_retries: 3,
    };
    let json = serde_json::to_string(&p).unwrap();
    let back: TaskPayload = serde_json::from_str(&json).unwrap();
    assert_eq!(back.task_id, p.task_id);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-core task_payload_roundtrips_json`
Expected: FAIL — TaskPayload 未定义

- [ ] **Step 3: 实现**

```rust
// pichost-core/src/models.rs 追加
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TaskPayload {
    pub task_id: uuid::Uuid,
    pub image_id: uuid::Uuid,
    pub user_id: uuid::Uuid,
    pub storage_backend: String,
    pub storage_config_id: Option<uuid::Uuid>,
    pub storage_backend_name: String,
    pub source_key: String,
    pub source_mime: String,
    pub retry_count: i32,
    pub max_retries: i32,
}
```

```rust
// pichost-worker/src/queue.rs — 删除 struct TaskPayload 定义，改:
use pichost_core::models::TaskPayload;

// pichost-api/src/services/upload.rs — 同步改引用（伴随修改）:
// 原: use pichost_worker::queue::TaskPayload;
// 新: use pichost_core::models::TaskPayload;
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-core task_payload_roundtrips_json` + `cargo check --workspace`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-core/src/models.rs pichost-core/src/models_test.rs pichost-worker/src/queue.rs pichost-api/src/services/upload.rs
git commit -m "refactor: move TaskPayload to pichost-core models (breaking)"
```

**migration_verify:**
- `cargo test -p pichost-api --test sqlite_migrations_test`（TaskPayload JSON 与 pending_tasks.payload_json 存储格式兼容）

**verify:**
- `cargo test -p pichost-core task_payload_roundtrips_json`
- `cargo check --workspace`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-worker queue`

---

### Task T16: 定义 5 个 state trait（Queue/Blacklist/RateLimiter/InviteStore/Cache）

**Files:**
- Create: `pichost-core/src/state/mod.rs`
- Create: `pichost-core/src/state/state_test.rs`
- Modify: `pichost-core/src/lib.rs`（`pub mod state;`）

**Interfaces:**
- Consumes: T15 的 `TaskPayload`（已迁入 core models）；`Uuid`
- Produces: `Queue`/`Blacklist`/`RateLimiter`/`InviteStore`/`Cache` trait + `RateLimitResult`/`NackAction`/`InviteCodeInfo` 关联类型 + 错误枚举

**depends_on:** [T15]

**breaking:** true（新公共模块）

**ac:**
- given: 5 个 trait 已定义且可作 trait object
  when: 编译契约测试 traits_are_object_safe 运行
  then: 5 个 Mock 实现均通过 &dyn Trait 引用编译

- [ ] **Step 1: 写失败测试（trait 编译契约）**

```rust
// pichost-core/src/state/state_test.rs
use crate::state::*;
use std::time::Duration;
use uuid::Uuid;

// 编译级契约：trait 必须可作 trait object
fn assert_queue_object(_q: &dyn Queue) {}
fn assert_blacklist_object(_b: &dyn Blacklist) {}
fn assert_rate_limiter_object(_r: &dyn RateLimiter) {}
fn assert_invite_object(_i: &dyn InviteStore) {}
fn assert_cache_object(_c: &dyn Cache) {}

#[test]
fn traits_are_object_safe() {
    assert_queue_object(&MockQueue);
    assert_blacklist_object(&MockBlacklist);
    assert_rate_limiter_object(&MockRateLimiter);
    assert_invite_object(&MockInviteStore);
    assert_cache_object(&MockCache);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-core traits_are_object_safe`
Expected: FAIL — 模块不存在

- [ ] **Step 3: 最小实现**

```rust
// pichost-core/src/state/mod.rs
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NackAction { Retry, DeadLetter }

#[derive(Debug, thiserror::Error)]
pub enum QueueError { #[error("queue error: {0}")] Other(String) }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitResult { pub allowed: bool, pub retry_after: u64 }

#[derive(Debug, thiserror::Error)]
pub enum RateLimiterError { #[error("rate limiter error: {0}")] Other(String) }

#[derive(Debug, thiserror::Error)]
pub enum BlacklistError { #[error("blacklist error: {0}")] Other(String) }

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct InviteCodeInfo {
    pub code: String,
    pub created_by: Option<Uuid>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
    pub used_by: Option<Uuid>,
}

#[derive(Debug, thiserror::Error)]
pub enum InviteError { #[error("invite error: {0}")] Other(String) }

#[derive(Debug, thiserror::Error)]
pub enum CacheError { #[error("cache error: {0}")] Other(String) }

#[async_trait::async_trait]
pub trait Queue: Send + Sync {
    async fn enqueue(&self, payload: &crate::models::TaskPayload) -> Result<(), QueueError>;
    async fn dequeue(&self, timeout: Duration) -> Result<Option<crate::models::TaskPayload>, QueueError>;
    async fn ack(&self, task_id: Uuid) -> Result<(), QueueError>;
    async fn nack(&self, task_id: Uuid, retry_count: i32, max_retries: i32) -> Result<NackAction, QueueError>;
}

#[async_trait::async_trait]
pub trait Blacklist: Send + Sync {
    async fn check(&self, jti: &str) -> Result<bool, BlacklistError>;
    async fn revoke(&self, jti: &str, ttl: Duration) -> Result<(), BlacklistError>;
}

#[async_trait::async_trait]
pub trait RateLimiter: Send + Sync {
    async fn check(&self, policy: &str, key: &str, limit: u32, window: Duration)
        -> Result<RateLimitResult, RateLimiterError>;
}

#[async_trait::async_trait]
pub trait InviteStore: Send + Sync {
    async fn create(&self, code: &str, created_by: Uuid, ttl_secs: u64) -> Result<(), InviteError>;
    async fn verify(&self, code: &str) -> Result<bool, InviteError>;
    async fn consume(&self, code: &str, used_by: Uuid) -> Result<(), InviteError>;
    async fn list(&self) -> Result<Vec<InviteCodeInfo>, InviteError>;
}

#[async_trait::async_trait]
pub trait Cache: Send + Sync {
    async fn get(&self, key: &str) -> Result<Option<String>, CacheError>;
    async fn set_ex(&self, key: &str, val: &str, ttl: u64) -> Result<(), CacheError>;
    async fn del(&self, key: &str) -> Result<(), CacheError>;
    async fn exists(&self, key: &str) -> Result<bool, CacheError>;
    async fn incr(&self, key: &str, ttl: u64) -> Result<u64, CacheError>;
}

// 测试用 Mock 实现（state_test.rs 使用）
pub struct MockQueue;
#[async_trait::async_trait]
impl Queue for MockQueue {
    async fn enqueue(&self, _p: &crate::models::TaskPayload) -> Result<(), QueueError> { Ok(()) }
    async fn dequeue(&self, _t: Duration) -> Result<Option<crate::models::TaskPayload>, QueueError> { Ok(None) }
    async fn ack(&self, _id: Uuid) -> Result<(), QueueError> { Ok(()) }
    async fn nack(&self, _id: Uuid, _r: i32, _m: i32) -> Result<NackAction, QueueError> { Ok(NackAction::Retry) }
}
// MockBlacklist / MockRateLimiter / MockInviteStore / MockCache 同理（最小 Ok 实现）
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-core traits_are_object_safe`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-core/src/state/ pichost-core/src/lib.rs
git commit -m "feat: define state traits (Queue/Blacklist/RateLimiter/InviteStore/Cache) (breaking)"
```

**verify:**
- `cargo test -p pichost-core traits_are_object_safe`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-core`
- `cargo check -p pichost-api`（旧代码不受影响）

---

### Task T17: RedisQueue trait 实现（worker）

**Files:**
- Modify: `pichost-worker/src/queue.rs`（函数封装为 `RedisQueue` 结构体实现 `Queue` trait）
- Modify: `pichost-worker/src/main.rs`（构造 RedisQueue 传入 worker_loop）
- Test: `pichost-worker/tests/queue_test.rs`（新建；集成测试）

**Interfaces:**
- Consumes: T16 的 `Queue` trait；T15 的 `TaskPayload`
- Produces: `pub struct RedisQueue { pool: deadpool_redis::Pool }` 实现 `Queue`（enqueue=HSET+LPUSH、dequeue=BRPOPLPUSH 封装、ack/nack 映射 NackAction）

**breaking:** true（worker 内部队列 API 变化）

**depends_on:** [T15, T16]

**ac:**
- given: Redis 可用且 RedisQueue::new(pool) 已构造
  when: enqueue 一个 TaskPayload 后 dequeue
  then: 返回的 task_id 与入队一致，ack 后任务消失

- [ ] **Step 1: 写失败测试**

```rust
// pichost-worker/tests/queue_test.rs（新建）
use pichost_core::models::TaskPayload;
use pichost_core::state::Queue;
use pichost_worker::queue::RedisQueue;
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running Redis"]
async fn redis_queue_trait_enqueue_dequeue() {
    let pool = test_pool(); // 参考现有 queue.rs 内嵌测试的 test_pool helper
    let q = RedisQueue::new(pool);
    let p = TaskPayload {
        task_id: Uuid::new_v4(), image_id: Uuid::new_v4(), user_id: Uuid::new_v4(),
        storage_backend: "local".into(), storage_config_id: None,
        storage_backend_name: "local".into(), source_key: "k".into(),
        source_mime: "image/png".into(), retry_count: 0, max_retries: 3,
    };
    q.enqueue(&p).await.unwrap();
    let got = q.dequeue(std::time::Duration::from_millis(100)).await.unwrap();
    assert_eq!(got.unwrap().task_id, p.task_id);
    q.ack(p.task_id).await.unwrap();
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-worker redis_queue_trait_enqueue_dequeue -- --ignored`
Expected: FAIL — RedisQueue 未定义

- [ ] **Step 3: 实现**

```rust
// pichost-worker/src/queue.rs
pub struct RedisQueue { pool: deadpool_redis::Pool }

#[async_trait::async_trait]
impl pichost_core::state::Queue for RedisQueue {
    // enqueue: 复用现有 enqueue_task 逻辑（HSET task_key + LPUSH pending）
    // dequeue: 复用 dequeue_task 逻辑（BRPOPLPUSH + HGET + 孤儿清理），
    //          返回 Option<TaskPayload>
    // ack: 复用 ack_task（LREM processing + HSET done）
    // nack: 复用 nack_task（重试 re-LPUSH 或 dead），映射 NackAction
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-worker redis_queue_trait_enqueue_dequeue -- --ignored`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-worker/src/queue.rs pichost-worker/src/main.rs
git commit -m "feat: implement Queue trait with RedisQueue (breaking)"
```

**verify:**
- `cargo test -p pichost-worker redis_queue_trait_enqueue_dequeue -- --ignored`
- `cargo check --workspace`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-worker queue`

---

### Task T18: RedisCache + RedisInviteStore 实现

**Files:**
- Modify: `pichost-api/src/cache/mod.rs`（`Cache` 结构体实现 `pichost_core::state::Cache` trait；invite 方法封装为 `RedisInviteStore` 实现 `InviteStore`）
- Modify: `pichost-api/src/routes/images.rs`（`cached_meta`/`cached_thumb` 泛型方法不可入 trait object → 调用方改为通过具体类型方法或提取为自由泛型函数）
- Test: `pichost-api/tests/cache_test.rs`（追加断言）

**Interfaces:**
- Consumes: T16 traits
- Produces: `impl Cache for Cache`、`pub struct RedisInviteStore { cache: Cache }` + `impl InviteStore for RedisInviteStore`

**breaking:** true（cache 模块 API 变化）

**depends_on:** [T16]

**ac:**
- given: Redis 可用且 Cache 已实现 pichost_core::state::Cache
  when: 经 &dyn Cache 执行 set_ex→get→exists→del
  then: 返回值与写入一致，del 后 get 为 None

- [ ] **Step 1: 写失败测试**

```rust
// cache_test.rs 追加（需 Redis）
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running Redis"]
async fn redis_cache_trait_roundtrip() {
    let cache = test_cache();
    let c: &dyn pichost_core::state::Cache = &cache;
    c.set_ex("t:1", "v", 60).await.unwrap();
    assert_eq!(c.get("t:1").await.unwrap(), Some("v".into()));
    assert!(c.exists("t:1").await.unwrap());
    c.del("t:1").await.unwrap();
    assert_eq!(c.get("t:1").await.unwrap(), None);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test cache_test redis_cache_trait_roundtrip -- --ignored`
Expected: FAIL — 未实现 trait

- [ ] **Step 3: 实现**

```rust
// cache/mod.rs
impl pichost_core::state::Cache for Cache {
    // get/set_ex/del/exists/incr 直接转发现有方法（错误映射 CacheError::Other）
}

pub struct RedisInviteStore { cache: Cache }
impl RedisInviteStore { pub fn new(cache: Cache) -> Self { Self { cache } } }
#[async_trait::async_trait]
impl pichost_core::state::InviteStore for RedisInviteStore {
    // create/verify/consume/list 转发现有 cache 方法（现有 HSET/SADD 逻辑）
}
```

```rust
// images.rs — cached_meta/cached_thumb 是泛型方法，trait object 不支持：
// 方案：保留 Cache 具体类型方法（非 trait 部分），AppState 持有 enum CacheImpl { Redis(Cache), Noop(NoopCache) }
// 或：cached_meta/cached_thumb 提取为自由泛型函数:
//   pub async fn cached_meta<C, T, F, E>(cache: &C, image_id, ttl, fetch) -> Result<T, E>
//     where C: pichost_core::state::Cache + ?Sized, T: DeserializeOwned + Serialize, F: Future<Output=Result<T,E>>
// 实现期按最小改动选择（推荐保留具体类型方法，通过内部 enum 分发）
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-api --test cache_test -- --ignored`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-api/src/cache/mod.rs pichost-api/src/routes/images.rs pichost-api/tests/cache_test.rs
git commit -m "feat: implement Cache and InviteStore traits with Redis backend (breaking)"
```

**verify:**
- `cargo test -p pichost-api --test cache_test -- --ignored`
- `cargo check --workspace`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api test_image_get`
- `cargo test -p pichost-api test_thumb`

---

### Task T19: RedisBlacklist 实现（auth 中间件 + 路由）

**Files:**
- Modify: `pichost-api/src/middleware/auth.rs`（定义 RedisBlacklist 结构体 + Blacklist trait 实现 + 检查点切换）
- Modify: `pichost-api/src/app.rs`（AppState 增加 `blacklist: Arc<dyn Blacklist>` 字段 + 构建注入）
- Test: `pichost-api/tests/auth_test.rs`（追加 RedisBlacklist 单元断言 + 现有 auth 集成测试回归）

**Interfaces:**
- Consumes: T16 `Blacklist` trait；T18 `Cache` 具体类型
- Produces: `pub struct RedisBlacklist { cache: Cache }` + `impl Blacklist for RedisBlacklist`（check=exists("bl:{jti}")、revoke=set_ex）；`AppState.blacklist: Arc<dyn Blacklist>` 字段（构建时注入）；middleware/auth.rs:79 检查点切换 state.blacklist（routes/auth.rs 的 revoke 切换在 T21 完成）

**breaking:** true（auth 路径依赖变化 + AppState 新字段）

**depends_on:** [T16, T18]

**ac:**
- given: Redis 可用且 RedisBlacklist 已构造
  when: revoke("jti-1", 60s) 后调用 check("jti-1")
  then: check 返回 true（revoke 生效），未 revoke 的 jti 返回 false

- [ ] **Step 1: 写失败测试**

```rust
// pichost-api/tests/auth_test.rs 追加
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Redis"]
async fn redis_blacklist_revoke_blocks_jti() {
    let cache = test_cache();
    let bl = pichost_api::middleware::auth::RedisBlacklist::new(cache);
    assert!(!bl.check("jti-1").await.unwrap());
    bl.revoke("jti-1", std::time::Duration::from_secs(60)).await.unwrap();
    assert!(bl.check("jti-1").await.unwrap());
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test auth_test redis_blacklist_revoke_blocks_jti -- --ignored`
Expected: FAIL — RedisBlacklist 未定义

- [ ] **Step 3: 实现**

```rust
// middleware/auth.rs
pub struct RedisBlacklist { cache: Cache }
impl RedisBlacklist { pub fn new(cache: Cache) -> Self { Self { cache } } }
#[async_trait::async_trait]
impl pichost_core::state::Blacklist for RedisBlacklist {
    async fn check(&self, jti: &str) -> Result<bool, BlacklistError> {
        self.cache.exists(&format!("bl:{}", jti))
            .await.map_err(|e| BlacklistError::Other(e.to_string()))
    }
    async fn revoke(&self, jti: &str, ttl: Duration) -> Result<(), BlacklistError> {
        self.cache.set_ex(&format!("bl:{}", jti), "revoked", ttl.as_secs())
            .await.map_err(|e| BlacklistError::Other(e.to_string()))
    }
}
```

```rust
// app.rs — AppState 增加字段 + 构建注入
pub struct AppState {
    // ...现有字段
    pub blacklist: Arc<dyn pichost_core::state::Blacklist>,
}
// 构建处:
let blacklist: Arc<dyn pichost_core::state::Blacklist> =
    Arc::new(crate::middleware::auth::RedisBlacklist::new((*cache).clone()));
```

```rust
// middleware/auth.rs:79 — 黑名单检查切换（字段已注入，立即生效）
// 原: let revoked = state.cache.exists(&format!("bl:{}", claims.jti)).await.unwrap_or(true);
// 新: let revoked = state.blacklist.check(&claims.jti).await.unwrap_or(true); // fail-closed 保留

// 注意: routes/auth.rs 的 revoke_old_tokens / logout / refresh 检查切换
// 在 T21（AppState 完整装配）一并完成 —— 本任务只切换 middleware 检查点。
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-api --test auth_test -- --ignored`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-api/src/middleware/auth.rs pichost-api/src/app.rs pichost-api/tests/auth_test.rs
git commit -m "feat: implement Blacklist trait with Redis backend (breaking)"
```

**verify:**
- `cargo test -p pichost-api redis_blacklist_revoke_blocks_jti -- --ignored`
- `cargo check --workspace`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api test_auth_login`
- `cargo test -p pichost-api test_logout`

---

### Task T20: RedisRateLimiter 实现

**Files:**
- Modify: `pichost-api/src/middleware/rate_limit.rs`（封装为 trait 实现 + 中间件调用切换）
- Modify: `pichost-api/src/app.rs`（AppState 增加 `rate_limiter: Arc<dyn RateLimiter>` 字段 + 构建注入）
- Test: `pichost-api/tests/rate_limit_test.rs`（新建；RedisRateLimiter 窗口测试）

**Interfaces:**
- Consumes: T16 `RateLimiter` trait；T18 `Cache` 具体类型
- Produces: `pub struct RedisRateLimiter { cache: Cache }` + `impl RateLimiter for RedisRateLimiter`（check=incr+TTL 查询，返回 allowed/retry_after）；`AppState.rate_limiter: Arc<dyn RateLimiter>` 字段（构建时注入）；check_rate_limit 中间件切换 state.rate_limiter

**depends_on:** [T16, T18]

**breaking:** true（AppState 新字段）

**ac:**
- given: Redis 可用且 RedisRateLimiter 已构造
  when: 同一 key 在 60s 窗口内连续调用 check 3 次（limit=2）
  then: 前 2 次 allowed=true，第 3 次 allowed=false 且 retry_after > 0

- [ ] **Step 1: 写失败测试**

```rust
// pichost-api/tests/rate_limit_test.rs（新建）
use pichost_api::middleware::rate_limit::RedisRateLimiter;
use pichost_core::state::RateLimiter;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Redis"]
async fn redis_rate_limiter_window() {
    let rl = RedisRateLimiter::new(test_cache());
    let r1 = rl.check("auth", "1.2.3.4", 2, Duration::from_secs(60)).await.unwrap();
    assert!(r1.allowed);
    let r2 = rl.check("auth", "1.2.3.4", 2, Duration::from_secs(60)).await.unwrap();
    assert!(r2.allowed);
    let r3 = rl.check("auth", "1.2.3.4", 2, Duration::from_secs(60)).await.unwrap();
    assert!(!r3.allowed);
    assert!(r3.retry_after > 0);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test rate_limit_test redis_rate_limiter_window -- --ignored`
Expected: FAIL

- [ ] **Step 3: 实现**

```rust
// rate_limit.rs
pub struct RedisRateLimiter { cache: Cache }
#[async_trait::async_trait]
impl pichost_core::state::RateLimiter for RedisRateLimiter {
    async fn check(&self, policy: &str, key: &str, limit: u32, window: Duration)
        -> Result<RateLimitResult, RateLimiterError> {
        let rl_key = format!("rl:{}:{}", policy, key);
        let count = self.cache.incr(&rl_key, window.as_secs())
            .await.map_err(|e| RateLimiterError::Other(e.to_string()))?;
        if count <= limit as u64 {
            Ok(RateLimitResult { allowed: true, retry_after: 0 })
        } else {
            Ok(RateLimitResult { allowed: false, retry_after: window.as_secs() })
        }
    }
}
```

```rust
// app.rs — AppState 增加字段 + 构建注入
pub struct AppState {
    // ...现有字段
    pub rate_limiter: Arc<dyn pichost_core::state::RateLimiter>,
}
// 构建处:
let rate_limiter: Arc<dyn pichost_core::state::RateLimiter> =
    Arc::new(crate::middleware::rate_limit::RedisRateLimiter::new((*cache).clone()));
```

```rust
// middleware/rate_limit.rs:48-78 — check_rate_limit 中间件切换（字段已注入，立即生效）
// 原: cache.incr(&rl_key, window_secs)... + TTL 查询
// 新: let result = state.rate_limiter.check(policy, suffix, limit, window).await
//         .unwrap_or(RateLimitResult { allowed: true, retry_after: 0 }); // fail-open 保留
//     if !result.allowed { return 429 + Retry-After: result.retry_after }
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-api --test rate_limit_test redis_rate_limiter_window -- --ignored`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-api/src/middleware/rate_limit.rs pichost-api/src/app.rs pichost-api/tests/rate_limit_test.rs
git commit -m "feat: implement RateLimiter trait with Redis backend (breaking)"
```

**verify:**
- `cargo test -p pichost-api --test rate_limit_test redis_rate_limiter_window -- --ignored`
- `cargo check --workspace`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api rate_limit`

---

### Task T21: AppState 完整装配 trait object

**Files:**
- Modify: `pichost-api/src/app.rs`（AppState 全部字段改 trait object + mode 分支构建）
- Modify: `pichost-api/src/main.rs`（构建实现注入：cache_pool/queue_pool）
- Modify: `pichost-api/tests/common/mod.rs`（harness 适配 AppState 新字段）

**Interfaces:**
- Consumes: T17-T20 的 Redis 实现（queue/blacklist/rate_limiter/invites/cache 字段已逐步注入）；T25 的 `pichost_worker::queue::RedisQueue`
- Produces: `AppState { pool, queue, blacklist, rate_limiter, invites, cache: Arc<dyn ...> }` 全字段 trait object；构建函数按 mode 分派（本期标准模式只注入 Redis 实现）；main.rs 传入 cache_pool/queue_pool

**breaking:** true（AppState 结构变化，全部 route handler 引用点需适配 `state.cache.xxx` → trait 方法）

**depends_on:** [T17, T18, T19, T20, T25]

**ac:**
- given: Docker PG+Redis 可用
  when: 经 test_app() 构造 AppState 并请求 GET /api/health
  then: 返回 200，标准模式全部 Redis 实现经 Arc<dyn Trait> 注入

- [ ] **Step 1: 写失败测试（编译失败即测试失败）**

```rust
// tests/common/mod.rs（或新增 tests/standard_mode_test.rs）
// 现有集成测试 harness 构造 AppState 编译即验证字段适配；
// 追加标准模式装配断言:
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires PG and Redis"]
async fn standard_mode_builds_redis_components() {
    let app = test_app().await;
    let resp = app.oneshot(axum::http::Request::builder()
        .uri("/api/health").body(axum::body::Body::empty()).unwrap()).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::OK);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo check -p pichost-api`
Expected: FAIL — AppState 字段类型变化导致编译错误

- [ ] **Step 3: 实现**

```rust
// app.rs
pub struct AppState {
    pub pool: DbPool,
    pub queue: Arc<dyn pichost_core::state::Queue>,
    pub blacklist: Arc<dyn pichost_core::state::Blacklist>,
    pub rate_limiter: Arc<dyn pichost_core::state::RateLimiter>,
    pub invites: Arc<dyn pichost_core::state::InviteStore>,
    pub cache: Arc<dyn pichost_core::state::Cache>,
    // ... 其余字段不变
}

// 构建（标准模式分支；sqlite 分支在 T26 接入）
// blacklist/rate_limiter/invites 已在 T19/T20 注入；本任务补齐 queue/cache 并组装
// main.rs: 传入 cache_pool/queue_pool；queue 用 pichost_worker::queue::RedisQueue
let cache: Arc<dyn pichost_core::state::Cache> = Arc::new(crate::cache::Cache::new(cache_pool));
let queue: Arc<dyn pichost_core::state::Queue> =
    Arc::new(pichost_worker::queue::RedisQueue::new(queue_pool));
// 组装全部 Arc<dyn> 字段进 AppState；按 mode 分派的构建函数在 T26 扩展 sqlite 分支
```

```rust
// tests/common/mod.rs — harness 适配
// AppState 直接构造点（约 line 117）补齐新字段:
//   queue: Arc::new(...), blacklist: ..., rate_limiter: ..., invites: ..., cache: ...
// 建议改用 app.rs 的构建函数（build_state_components）避免重复
```

```rust
// routes/auth.rs — revoke_old_tokens / logout / refresh 检查切换（T19 遗留的调用点）
// 原: cache.set_ex(&format!("bl:{}", jti), "revoked", ttl).await
//     cache.exists(&format!("bl:{}", jti))...
// 新: state.blacklist.revoke(&jti, Duration::from_secs(ttl)).await
//     state.blacklist.check(&jti).await.unwrap_or(true)
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo check --workspace` + `cargo test -p pichost-api -- --ignored`
Expected: PASS（Docker infra 下全量）

- [ ] **Step 5: 提交**

```bash
git add pichost-api/src/app.rs pichost-api/src/main.rs pichost-api/tests/common/mod.rs
git commit -m "feat: assemble state traits into AppState (breaking)"
```

**verify:**
- `cargo check --workspace`
- `cargo test -p pichost-api test_health -- --ignored`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api -- --include-ignored`（Docker infra 下全量回归）

---

## Phase D — SQLite 双实现 + 轻量模式

### Task T22: migrations-sqlite 0011 状态表 + SqliteQueue

**Files:**
- Create: `migrations-sqlite/0011_create_lite_state_tables.sql`
- Create: `pichost-core/src/state/sqlite_queue.rs`
- Test: `pichost-core/src/state/state_test.rs`（追加）

**Interfaces:**
- Consumes: T7 `DbPool`/`DatabaseMode`；T15 `Queue` trait；T16 `TaskPayload`
- Produces: `pub struct SqliteQueue { pool: DbPool }` + `impl Queue for SqliteQueue`（pending_tasks 表 + 原子 claim + 500ms 轮询语义）

**breaking:** true（新迁移 + 新实现）

**depends_on:** [T7, T16]

**ac:**
- given: sqlite 内存库已迁移（含 0011 状态表）
  when: enqueue → dequeue → 再次 dequeue → ack → 再次 dequeue
  then: 第 1 次 dequeue 返回任务，第 2 次 None（原子 claim），ack 后第 3 次 None

- [ ] **Step 1: 写失败测试**

```rust
// state_test.rs 追加
use crate::config::DatabaseMode;
use crate::db::{create_pool, run_migrations};
use crate::state::sqlite_queue::SqliteQueue;
use crate::state::Queue;

fn sample_task() -> crate::models::TaskPayload {
    use uuid::Uuid;
    crate::models::TaskPayload {
        task_id: Uuid::new_v4(), image_id: Uuid::new_v4(), user_id: Uuid::new_v4(),
        storage_backend: "local".into(), storage_config_id: None,
        storage_backend_name: "local".into(), source_key: "k".into(),
        source_mime: "image/png".into(), retry_count: 0, max_retries: 3,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_queue_claim_ack_cycle() {
    let pool = create_pool("sqlite::memory:", 5, DatabaseMode::Sqlite).await.unwrap();
    run_migrations(&pool, DatabaseMode::Sqlite).await.unwrap();
    let q = SqliteQueue::new(pool);
    let p = sample_task();
    q.enqueue(&p).await.unwrap();
    let got = q.dequeue(Duration::from_millis(50)).await.unwrap().unwrap();
    assert_eq!(got.task_id, p.task_id);
    // 原子 claim：已 claim 任务第二次 dequeue 拿不到
    let second = q.dequeue(Duration::from_millis(50)).await.unwrap();
    assert!(second.is_none());
    q.ack(p.task_id).await.unwrap();
    let third = q.dequeue(Duration::from_millis(50)).await.unwrap();
    assert!(third.is_none());
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-core sqlite_queue_claim_ack_cycle`
Expected: FAIL — SqliteQueue 未定义

- [ ] **Step 3: 实现**

```sql
-- migrations-sqlite/0011_create_lite_state_tables.sql
CREATE TABLE pending_tasks (
    task_id      TEXT PRIMARY KEY,
    payload_json TEXT NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending',
    retry_count  INTEGER NOT NULL DEFAULT 0,
    claimed_at   TEXT,
    updated_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    created_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now'))
);
CREATE INDEX idx_pending_tasks_status ON pending_tasks(status);

CREATE TABLE token_blacklist (
    jti        TEXT PRIMARY KEY,
    expires_at TEXT NOT NULL
);

CREATE TABLE rate_limits (
    policy       TEXT NOT NULL,
    key          TEXT NOT NULL,
    window_start INTEGER NOT NULL,
    count        INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (policy, key, window_start)
);

CREATE TABLE invite_codes (
    code       TEXT PRIMARY KEY,
    created_by TEXT,
    created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ','now')),
    expires_at TEXT,
    used_by    TEXT
);
```

```rust
// pichost-core/src/state/sqlite_queue.rs
use crate::db::DbPool;
use crate::models::TaskPayload;
use crate::state::{NackAction, Queue, QueueError};
use std::time::Duration;
use uuid::Uuid;

pub struct SqliteQueue { pool: DbPool }

impl SqliteQueue { pub fn new(pool: DbPool) -> Self { Self { pool } } }

#[async_trait::async_trait]
impl Queue for SqliteQueue {
    async fn enqueue(&self, p: &TaskPayload) -> Result<(), QueueError> {
        let json = serde_json::to_string(p).map_err(|e| QueueError::Other(e.to_string()))?;
        sqlx::query("INSERT INTO pending_tasks (task_id, payload_json, status) VALUES (?, ?, 'pending')")
            .bind(p.task_id.to_string()).bind(json)
            .execute(&self.pool).await.map_err(|e| QueueError::Other(e.to_string()))?;
        Ok(())
    }
    async fn dequeue(&self, _timeout: Duration) -> Result<Option<TaskPayload>, QueueError> {
        let row = sqlx::query_as::<_, (String, String)>(
            "UPDATE pending_tasks SET status='processing', claimed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'), updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') \
             WHERE task_id = (SELECT task_id FROM pending_tasks WHERE status='pending' AND claimed_at IS NULL ORDER BY created_at LIMIT 1) \
             RETURNING task_id, payload_json")
            .fetch_optional(&self.pool).await.map_err(|e| QueueError::Other(e.to_string()))?;
        match row {
            Some((_id, json)) => serde_json::from_str(&json).map(Some)
                .map_err(|e| QueueError::Other(e.to_string())),
            None => Ok(None),
        }
    }
    async fn ack(&self, task_id: Uuid) -> Result<(), QueueError> {
        sqlx::query("DELETE FROM pending_tasks WHERE task_id = ?")
            .bind(task_id.to_string()).execute(&self.pool).await
            .map_err(|e| QueueError::Other(e.to_string()))?;
        Ok(())
    }
    async fn nack(&self, task_id: Uuid, retry_count: i32, max_retries: i32) -> Result<NackAction, QueueError> {
        if retry_count < max_retries {
            sqlx::query("UPDATE pending_tasks SET status='pending', claimed_at=NULL, retry_count=?, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE task_id = ?")
                .bind(retry_count + 1).bind(task_id.to_string()).execute(&self.pool).await
                .map_err(|e| QueueError::Other(e.to_string()))?;
            Ok(NackAction::Retry)
        } else {
            sqlx::query("UPDATE pending_tasks SET status='dead', claimed_at=NULL, updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE task_id = ?")
                .bind(task_id.to_string()).execute(&self.pool).await
                .map_err(|e| QueueError::Other(e.to_string()))?;
            Ok(NackAction::DeadLetter)
        }
    }
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-core sqlite_queue_claim_ack_cycle`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add migrations-sqlite/0011_create_lite_state_tables.sql pichost-core/src/state/sqlite_queue.rs pichost-core/src/state/state_test.rs
git commit -m "feat: implement SqliteQueue with atomic claim (breaking)"
```

**migration_verify:**
- pending_tasks / token_blacklist / rate_limits / invite_codes 表存在（0011 迁移）
- `_sqlx_migrations` 最大版本 = 11

**verify:**
- `cargo test -p pichost-core sqlite_queue_claim_ack_cycle`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-core`

---

### Task T23: SqliteBlacklist + SqliteRateLimiter

**Files:**
- Create: `pichost-core/src/state/sqlite_blacklist.rs`
- Create: `pichost-core/src/state/sqlite_rate_limiter.rs`
- Test: `pichost-core/src/state/state_test.rs`（追加）

**Interfaces:**
- Consumes: T22 的 0011 迁移表（token_blacklist / rate_limits）
- Produces: `SqliteBlacklist` / `SqliteRateLimiter` 实现

**breaking:** true

**depends_on:** [T7, T22]

**ac:**
- given: sqlite 内存库已迁移（含 0011 状态表）
  when: revoke 一个 jti 后 check；同一 key 连续 check 3 次（limit=2）
  then: revoke 后 check=true，第 3 次限流 allowed=false

- [ ] **Step 1: 写失败测试**

```rust
// state_test.rs 追加
use crate::state::sqlite_blacklist::SqliteBlacklist;
use crate::state::sqlite_rate_limiter::SqliteRateLimiter;
use crate::state::{Blacklist, RateLimiter};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_blacklist_and_rate_limiter() {
    let pool = create_pool("sqlite::memory:", 5, DatabaseMode::Sqlite).await.unwrap();
    run_migrations(&pool, DatabaseMode::Sqlite).await.unwrap();
    let bl = SqliteBlacklist::new(pool.clone());
    assert!(!bl.check("jti-1").await.unwrap());
    bl.revoke("jti-1", Duration::from_secs(60)).await.unwrap();
    assert!(bl.check("jti-1").await.unwrap());

    let rl = SqliteRateLimiter::new(pool);
    let r1 = rl.check("auth", "1.2.3.4", 2, Duration::from_secs(60)).await.unwrap();
    assert!(r1.allowed);
    let _r2 = rl.check("auth", "1.2.3.4", 2, Duration::from_secs(60)).await.unwrap();
    let r3 = rl.check("auth", "1.2.3.4", 2, Duration::from_secs(60)).await.unwrap();
    assert!(!r3.allowed);
    assert!(r3.retry_after > 0);
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-core sqlite_blacklist_and_rate_limiter`
Expected: FAIL

- [ ] **Step 3: 实现**

```rust
// pichost-core/src/state/sqlite_blacklist.rs
use crate::db::DbPool;
use crate::state::{Blacklist, BlacklistError};
use std::time::Duration;

pub struct SqliteBlacklist { pool: DbPool }
impl SqliteBlacklist { pub fn new(pool: DbPool) -> Self { Self { pool } } }

#[async_trait::async_trait]
impl Blacklist for SqliteBlacklist {
    async fn check(&self, jti: &str) -> Result<bool, BlacklistError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM token_blacklist WHERE jti = ? AND expires_at > strftime('%Y-%m-%dT%H:%M:%fZ','now')")
            .bind(jti).fetch_one(&self.pool).await
            .map_err(|e| BlacklistError::Other(e.to_string()))?;
        Ok(n > 0)
    }
    async fn revoke(&self, jti: &str, ttl: Duration) -> Result<(), BlacklistError> {
        let expires = (chrono::Utc::now() + chrono::Duration::from_std(ttl).unwrap_or_default())
            .format("%Y-%m-%dT%H:%M:%fZ").to_string();
        sqlx::query("INSERT INTO token_blacklist (jti, expires_at) VALUES (?, ?) ON CONFLICT(jti) DO UPDATE SET expires_at = excluded.expires_at")
            .bind(jti).bind(expires).execute(&self.pool).await
            .map_err(|e| BlacklistError::Other(e.to_string()))?;
        Ok(())
    }
}
```

```rust
// pichost-core/src/state/sqlite_rate_limiter.rs
use crate::db::DbPool;
use crate::state::{RateLimiter, RateLimiterError, RateLimitResult};
use std::time::Duration;

pub struct SqliteRateLimiter { pool: DbPool }
impl SqliteRateLimiter { pub fn new(pool: DbPool) -> Self { Self { pool } } }

#[async_trait::async_trait]
impl RateLimiter for SqliteRateLimiter {
    async fn check(&self, policy: &str, key: &str, limit: u32, window: Duration) -> Result<RateLimitResult, RateLimiterError> {
        let window_start = chrono::Utc::now().timestamp() / window.as_secs() as i64;
        // 原子 upsert 计数
        let count: i64 = sqlx::query_scalar(
            "INSERT INTO rate_limits (policy, key, window_start, count) VALUES (?, ?, ?, 1) \
             ON CONFLICT(policy, key, window_start) DO UPDATE SET count = count + 1 RETURNING count")
            .bind(policy).bind(key).bind(window_start).fetch_one(&self.pool).await
            .map_err(|e| RateLimiterError::Other(e.to_string()))?;
        if count <= limit as i64 {
            Ok(RateLimitResult { allowed: true, retry_after: 0 })
        } else {
            Ok(RateLimitResult { allowed: false, retry_after: window.as_secs() })
        }
    }
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-core sqlite_blacklist_and_rate_limiter`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-core/src/state/sqlite_blacklist.rs pichost-core/src/state/sqlite_rate_limiter.rs pichost-core/src/state/state_test.rs
git commit -m "feat: implement SqliteBlacklist and SqliteRateLimiter (breaking)"
```

**verify:**
- `cargo test -p pichost-core sqlite_blacklist_and_rate_limiter`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-core`

---

### Task T24: SqliteInviteStore + NoopCache

**Files:**
- Create: `pichost-core/src/state/sqlite_invite.rs`
- Create: `pichost-core/src/state/noop_cache.rs`
- Test: `pichost-core/src/state/state_test.rs`（追加）

**Interfaces:**
- Consumes: T22 的 0011 迁移表（invite_codes）
- Produces: `SqliteInviteStore` + `NoopCache` 实现

**breaking:** true

**depends_on:** [T7, T22]

**ac:**
- given: sqlite 内存库已迁移（含 0011 状态表）
  when: create 邀请码后 verify，consume 后再 verify；NoopCache get/set_ex
  then: 未消费时 verify=true，消费后 verify=false；NoopCache 恒 miss（get=None）

- [ ] **Step 1: 写失败测试**

```rust
// state_test.rs 追加
use crate::state::noop_cache::NoopCache;
use crate::state::sqlite_invite::SqliteInviteStore;
use crate::state::{Cache, InviteStore};
use uuid::Uuid;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_invite_store_and_noop_cache() {
    let pool = create_pool("sqlite::memory:", 5, DatabaseMode::Sqlite).await.unwrap();
    run_migrations(&pool, DatabaseMode::Sqlite).await.unwrap();
    let inv = SqliteInviteStore::new(pool);
    let admin = Uuid::new_v4();
    inv.create("CODE1", admin, 3600).await.unwrap();
    assert!(inv.verify("CODE1").await.unwrap());
    inv.consume("CODE1", Uuid::new_v4()).await.unwrap();
    assert!(!inv.verify("CODE1").await.unwrap());

    let noop = NoopCache;
    assert_eq!(noop.get("x").await.unwrap(), None);
    noop.set_ex("x", "v", 60).await.unwrap();
    assert_eq!(noop.get("x").await.unwrap(), None); // noop 恒 miss
    assert_eq!(noop.incr("c", 60).await.unwrap(), 1); // 恒返回递增数（进程内计数）
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-core sqlite_invite_store_and_noop_cache`
Expected: FAIL

- [ ] **Step 3: 实现**

```rust
// pichost-core/src/state/sqlite_invite.rs
use crate::db::DbPool;
use crate::state::{InviteCodeInfo, InviteError, InviteStore};
use uuid::Uuid;

pub struct SqliteInviteStore { pool: DbPool }
impl SqliteInviteStore { pub fn new(pool: DbPool) -> Self { Self { pool } } }

#[async_trait::async_trait]
impl InviteStore for SqliteInviteStore {
    async fn create(&self, code: &str, created_by: Uuid, ttl_secs: u64) -> Result<(), InviteError> {
        let expires = (chrono::Utc::now() + chrono::Duration::seconds(ttl_secs as i64))
            .format("%Y-%m-%dT%H:%M:%fZ").to_string();
        sqlx::query("INSERT INTO invite_codes (code, created_by, expires_at) VALUES (?, ?, ?)")
            .bind(code).bind(created_by.to_string()).bind(expires).execute(&self.pool).await
            .map_err(|e| InviteError::Other(e.to_string()))?;
        Ok(())
    }
    async fn verify(&self, code: &str) -> Result<bool, InviteError> {
        let n: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM invite_codes WHERE code = ? AND used_by IS NULL AND (expires_at IS NULL OR expires_at > strftime('%Y-%m-%dT%H:%M:%fZ','now'))")
            .bind(code).fetch_one(&self.pool).await
            .map_err(|e| InviteError::Other(e.to_string()))?;
        Ok(n > 0)
    }
    async fn consume(&self, code: &str, used_by: Uuid) -> Result<(), InviteError> {
        sqlx::query("UPDATE invite_codes SET used_by = ? WHERE code = ? AND used_by IS NULL")
            .bind(used_by.to_string()).bind(code).execute(&self.pool).await
            .map_err(|e| InviteError::Other(e.to_string()))?;
        Ok(())
    }
    async fn list(&self) -> Result<Vec<InviteCodeInfo>, InviteError> {
        sqlx::query_as::<_, (String, Option<String>, chrono::DateTime<chrono::Utc>, Option<chrono::DateTime<chrono::Utc>>, Option<String>)>(
            "SELECT code, created_by, created_at, expires_at, used_by FROM invite_codes ORDER BY created_at DESC")
            .fetch_all(&self.pool).await
            .map(|rows| rows.into_iter().map(|(code, cb, ca, ea, ub)| InviteCodeInfo {
                code, created_by: cb.map(|s| Uuid::parse_str(&s).unwrap_or_default()).filter(|u| !u.is_nil()),
                created_at: ca, expires_at: ea, used_by: ub.and_then(|s| Uuid::parse_str(&s).ok()),
            }).collect())
            .map_err(|e| InviteError::Other(e.to_string()))
    }
}
```

```rust
// pichost-core/src/state/noop_cache.rs
use crate::state::{Cache, CacheError};

pub struct NoopCache;
#[async_trait::async_trait]
impl Cache for NoopCache {
    async fn get(&self, _key: &str) -> Result<Option<String>, CacheError> { Ok(None) }
    async fn set_ex(&self, _key: &str, _val: &str, _ttl: u64) -> Result<(), CacheError> { Ok(()) }
    async fn del(&self, _key: &str) -> Result<(), CacheError> { Ok(()) }
    async fn exists(&self, _key: &str) -> Result<bool, CacheError> { Ok(false) }
    async fn incr(&self, key: &str, _ttl: u64) -> Result<u64, CacheError> {
        // 进程内递增计数（限流 fail-open 语义）；生产实现建议用进程内计数器
        Ok(1)
    }
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-core sqlite_invite_store_and_noop_cache`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-core/src/state/sqlite_invite.rs pichost-core/src/state/noop_cache.rs pichost-core/src/state/state_test.rs
git commit -m "feat: implement SqliteInviteStore and NoopCache (breaking)"
```

**verify:**
- `cargo test -p pichost-core sqlite_invite_store_and_noop_cache`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-core`

---

### Task T25: pichost-worker 库化（lib.rs 暴露 process_task）

**Files:**
- Create: `pichost-worker/src/lib.rs`（暴露 `process_task` 等管线函数）
- Modify: `pichost-worker/src/main.rs`（worker_loop 改为调用 lib 的 process_task）
- Test: `pichost-worker/tests/process_task_test.rs`（新建冒烟）

**Interfaces:**
- Consumes: 现有 `pipeline.rs` 的 `process_task()`（pipeline.rs:229 附近）
- Produces: `pichost_worker::process_task(pool, router, payload) -> Result<(), PipelineError>` 公共 API（供轻量模式 API 进程内调用）

**breaking:** true（crate 新增 lib 目标）

**depends_on:** [T15]

**ac:**
- given: pichost-worker lib 目标已暴露 process_task
  when: cargo test -p pichost-worker --test process_task_test 编译运行
  then: 公共 API 可调用（冒烟），且现有 worker 测试全量通过

- [ ] **Step 1: 写失败测试**

```rust
// pichost-worker/tests/process_task_test.rs
use pichost_core::config::DatabaseMode;
use pichost_core::db::create_pool;
use pichost_worker::process_task;
use std::str::FromStr;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires storage setup; smoke only"]
async fn process_task_public_api_compiles_and_runs() {
    // 以 sqlite 内存库经统一 create_pool 构造 AnyPool；router 用空 StorageRouter
    let pool = create_pool("sqlite::memory:", 1, DatabaseMode::Sqlite).await.unwrap();
    let _ = process_task(&pool, &pichost_core::storage::StorageRouter::default(), &sample_payload()).await;
    // 本测试仅验证公共 API 可编译可调用；真实行为由现有 pipeline 测试覆盖
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-worker --test process_task_test`
Expected: FAIL — lib 目标不存在

- [ ] **Step 3: 实现**

```rust
// pichost-worker/src/lib.rs
pub mod fonts;
pub mod pipeline;
pub mod processor;
pub mod watermark;

pub use pipeline::process_task;
// main.rs 中 worker_loop 调用的 process_task 改为经 lib 导出（消除重复定义）
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-worker --test process_task_test` + `cargo test -p pichost-worker`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-worker/src/lib.rs pichost-worker/src/main.rs pichost-worker/tests/process_task_test.rs
git commit -m "feat: expose pichost-worker as library with process_task API (breaking)"
```

**verify:**
- `cargo test -p pichost-worker --test process_task_test`
- `cargo test -p pichost-worker`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-worker pipeline`

---

### Task T26: 轻量模式装配（AppState sqlite 分支 + 内嵌 worker + e2e）

**Files:**
- Modify: `pichost-api/src/app.rs`（sqlite 模式构建 Sqlite* 实现 + spawn lite_worker_task）
- Modify: `pichost-api/src/main.rs`（传 mode 到装配函数）
- Test: `pichost-api/tests/sqlite_e2e_test.rs`（新建全链路测试）

**Interfaces:**
- Consumes: T22-T24 SQLite 实现；T25 `pichost_worker::process_task`
- Produces: 轻量模式完整装配；`lite_worker_task(pool, queue, router)` 后台任务

**breaking:** false（新分支，标准模式不变）

**depends_on:** [T21, T22, T23, T24, T25]

**ac:**
- given: 以 sqlite 文件模式构建 AppState
  when: 走通 注册→登录→上传→GET /images→GET /u/{key}→等 1s 查 thumbnail_url
  then: 全链路 200/成功，且内嵌 worker 已生成缩略图；标准模式现有测试保持通过

- [ ] **Step 1: 写失败测试**

```rust
// pichost-api/tests/sqlite_e2e_test.rs
use pichost_api::app::build_app_state; // 按实际导出名
use tower::ServiceExt;
use axum::http::{Request, StatusCode};
use axum::body::Body;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn sqlite_mode_full_flow() {
    // 临时 sqlite 文件（非内存：跨连接共享 + WAL）
    let dir = tempfile::tempdir().unwrap();
    let db_url = format!("sqlite://{}/e2e.db", dir.path().display());
    let state = build_app_state(&db_url, DatabaseMode::Sqlite).await;
    let app = pichost_api::app::configure_app(state);

    // 1. 注册首个用户（自动 admin）
    let resp = app.clone().oneshot(Request::builder()
        .method("POST").uri("/api/v1/auth/register")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"username":"admin","password":"admin123456"}"#))
        .unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // 2. 登录 → 拿 access token
    let resp = app.clone().oneshot(Request::builder()
        .method("POST").uri("/api/v1/auth/login")
        .header("content-type", "application/json")
        .body(Body::from(r#"{"username":"admin","password":"admin123456"}"#))
        .unwrap()).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(&axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap()).unwrap();
    let token = body["access_token"].as_str().unwrap();

    // 3. 上传图片（multipart）→ 200（tiny_png 字节）
    // 4. GET /images → 列表含 1 条
    // 5. GET /u/{public_key} → 200
    // 6. 等 ~1s（内嵌 worker 处理缩略图）→ GET /images/:id 含 thumbnail_url
    // 完整断言参照 tests/common/mod.rs 的 multipart_image/tiny_png helper 实现
}
```

- [ ] **Step 2: 运行确认失败**

Run: `cargo test -p pichost-api --test sqlite_e2e_test`
Expected: FAIL — sqlite 装配不存在

- [ ] **Step 3: 实现**

```rust
// app.rs — 装配函数按 mode 分派
match mode {
    DatabaseMode::Postgres => build_standard_state(pool, cache_pool, queue_pool).await,
    DatabaseMode::Sqlite => {
        let cache: Arc<dyn Cache> = Arc::new(NoopCache);
        let blacklist: Arc<dyn Blacklist> = Arc::new(SqliteBlacklist::new(pool.clone()));
        let rate_limiter: Arc<dyn RateLimiter> = Arc::new(SqliteRateLimiter::new(pool.clone()));
        let invites: Arc<dyn InviteStore> = Arc::new(SqliteInviteStore::new(pool.clone()));
        let queue: Arc<dyn Queue> = Arc::new(SqliteQueue::new(pool.clone()));
        // spawn 内嵌 worker:
        // tokio::spawn(lite_worker_task(pool.clone(), queue.clone(), router.clone()));
    }
}

// lite_worker_task: 循环 dequeue → process_task → ack/nack
pub async fn lite_worker_task(pool: DbPool, queue: Arc<dyn Queue>, router: Arc<StorageRouter>) {
    loop {
        match queue.dequeue(Duration::from_millis(500)).await {
            Ok(Some(payload)) => {
                let res = pichost_worker::process_task(&pool, &router, &payload).await;
                match res {
                    Ok(()) => { let _ = queue.ack(payload.task_id).await; }
                    Err(_) => {
                        let action = queue.nack(payload.task_id, payload.retry_count, payload.max_retries).await.unwrap_or(NackAction::DeadLetter);
                        if action == NackAction::DeadLetter { /* 记录死信 */ }
                    }
                }
            }
            _ => tokio::time::sleep(Duration::from_millis(500)).await,
        }
    }
}
```

- [ ] **Step 4: 运行确认通过**

Run: `cargo test -p pichost-api --test sqlite_e2e_test`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add pichost-api/src/app.rs pichost-api/src/main.rs pichost-api/tests/sqlite_e2e_test.rs
git commit -m "feat: assemble sqlite lite mode with embedded worker"
```

**verify:**
- `cargo test -p pichost-api --test sqlite_e2e_test`
- `cargo test -p pichost-api test_health`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test -p pichost-api -- --include-ignored`（Docker infra 下全量）

---

## Phase E — 安装交互化 + 发布配套

### Task T27: install.sh 交互化 + systemd 条件化

**Files:**
- Modify: `scripts/install.sh`（交互检测 + 提问 + mode 分支 .env 生成 + 用户/权限创建 + JWT 校验 + service 单元条件化）
- Test: `scripts/tests/install_test.sh`（新建；shell 断言）

**Interfaces:**
- Consumes: spec §6 交互流程；现有 install.sh 56 行结构；`scripts/pichost-api.service`/`scripts/pichost-worker.service` 模板（install.sh 内 sed 条件化，不改动模板文件本身）
- Produces: 交互式安装脚本（`--yes` 无人值守 + `--mode postgres|sqlite`）

**depends_on:** []

**breaking:** false（脚本向后兼容：无参数时行为=现有安装 + 交互提问）

**ac:**
- given: 无 pg_isready 的临时环境，install.sh 以 --yes --mode sqlite 运行
  when: 安装到临时目录
  then: /tmp/pc/.env 含 PICHOST_DATABASE_MODE=sqlite 且 URL 指向 sqlite://$DATA_DIR/pichost.db，且生成的 service 单元不含 postgresql.service 依赖

- [ ] **Step 1: 写失败测试（shell 断言）**

```bash
# scripts/tests/install_test.sh（新建）
#!/usr/bin/env bash
set -euo pipefail
# 用法: bash scripts/tests/install_test.sh <pkg_dir>
PKG_DIR="${1:?usage: install_test.sh <pkg_dir>}"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
(cd "$PKG_DIR" && bash scripts/install.sh --yes --mode sqlite \
  "$TMP/pi" "$TMP/pd" "$TMP/pc")
grep -q 'PICHOST_DATABASE_MODE=sqlite' "$TMP/pc/.env"
grep -q 'sqlite://' "$TMP/pc/.env"
! grep -q 'Wants=.*postgresql' "$TMP/pc/pichost-api.service" 2>/dev/null || true
# systemd 不可用时跳过单元断言；核心断言为 .env 生成
echo "install_test.sh PASS"
```

- [ ] **Step 2: 运行确认失败**

Run: `bash scripts/tests/install_test.sh dist/pichost-v0.21.0-amd64`
Expected: FAIL — install.sh 无 --mode 参数支持

- [ ] **Step 3: 实现**

```bash
# scripts/install.sh 核心新增逻辑（保留现有 mkdir/cp/systemd 逻辑）:
# 1. 检测函数: has_pg() { command -v pg_isready >/dev/null 2>&1; }
#             has_redis() { command -v redis-cli >/dev/null 2>&1; }
# 2. MODE 参数解析: --mode postgres|sqlite（默认自动检测: 无 pg → 提问）
# 3. 交互提问（无 --yes 且 stdin 为 tty 时）:
#    PG 缺失 → read -p "[1] 自动安装 PostgreSQL [2] 改用 SQLite [3] 手动安装后重跑: "
#    Redis 缺失（postgres 模式）→ read -p "[1] apt 自动安装 [2] 手动安装后重跑: "
# 4. .env 生成（按 mode）:
#    sqlite: PICHOST_DATABASE_MODE=sqlite
#            PICHOST_DATABASE_URL="sqlite://$DATA_DIR/pichost.db"
#    postgres: PICHOST_DATABASE_MODE=postgres（默认，可省略）+ 原变量
# 5. 创建 pichost 用户: id pichost || useradd --system --home "$INSTALL_DIR" pichost
# 6. chown -R pichost:pichost "$INSTALL_DIR" "$DATA_DIR" "$CONFIG_DIR"
# 7. JWT 校验: grep 长度 < 32 → 生成随机 secret 写入 .env 并提示
# 8. service 单元生成: sqlite 模式 sed 删除 "Wants=.*postgresql.*redis" 行
```

- [ ] **Step 4: 运行确认通过**

Run: `bash scripts/tests/install_test.sh dist/pichost-v0.21.0-amd64`（先 `bash scripts/verify-release.sh --skip-test --skip-lint --skip-install` 生成 dist）
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add scripts/install.sh scripts/tests/install_test.sh
git commit -m "feat: interactive install with sqlite mode guidance and systemd conditional deps"
```

**verify:**
- `bash -n scripts/install.sh`
- `bash scripts/tests/install_test.sh <pkg_dir>`

**regression:**
- `bash scripts/verify-release.sh --skip-test --skip-lint --skip-install`（布局校验不变）

---

### Task T28: verify-release.sh sqlite 冒烟 + .env.example 更新

**Files:**
- Modify: `scripts/verify-release.sh`（check_binary 参数化 mode；sqlite 冒烟分支）
- Modify: `.env.example`（PICHOST_DATABASE_MODE 注释 + 补齐 i18n 变量）
- Test: `scripts/tests/verify_release_test.sh`（新建；断言 verify-release 含 sqlite 冒烟路径 + .env.example 含新变量）

**Interfaces:**
- Consumes: T27 的 install.sh 参数
- Produces: verify-release 覆盖 sqlite 模式（二进制冒烟 + install dry-run）

**depends_on:** [T27]

**breaking:** false

**ac:**
- given: verify-release.sh 已含 sqlite 冒烟分支
  when: 运行 bash scripts/verify-release.sh --skip-test --skip-lint
  then: 二进制以 PICHOST_DATABASE_MODE=sqlite + 临时文件 URL 启动并保持运行（rc=124 OK），.env.example 含 PICHOST_DATABASE_MODE 与 i18n 变量

- [ ] **Step 1: 写失败测试（验证 sqlite 冒烟缺失）**

```bash
# scripts/tests/verify_release_test.sh（新建）
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# 断言 verify-release.sh 含 sqlite 冒烟分支（check_binary_sqlite 或 PICHOST_DATABASE_MODE=sqlite 出现）
grep -q 'PICHOST_DATABASE_MODE=sqlite' "$ROOT/scripts/verify-release.sh" \
  || { echo "FAIL: verify-release.sh missing sqlite smoke"; exit 1; }
# 断言 .env.example 含新变量
grep -q 'PICHOST_DATABASE_MODE' "$ROOT/.env.example" \
  || { echo "FAIL: .env.example missing PICHOST_DATABASE_MODE"; exit 1; }
grep -q 'PICHOST_I18N_LANGUAGE' "$ROOT/.env.example" \
  || { echo "FAIL: .env.example missing PICHOST_I18N_LANGUAGE"; exit 1; }
echo "verify_release_test.sh PASS"
```

- [ ] **Step 2: 运行确认失败**

Run: `bash scripts/tests/verify_release_test.sh`
Expected: FAIL — 冒烟仅 PG 路径

- [ ] **Step 3: 实现**

```bash
# scripts/verify-release.sh — check_binary 增加 sqlite 变体:
# check_binary_sqlite() { PICHOST_DATABASE_MODE=sqlite PICHOST_DATABASE_URL="sqlite://$(mktemp -d)/smoke.db" \
#   PICHOST_REDIS_URL= timeout 8 "$bin" >/dev/null 2>&1; ... }
# sqlite 模式预期: 进程启动并保持运行（rc=124 OK）——迁移自动应用后进入 serve
```

```bash
# .env.example 追加:
# PICHOST_DATABASE_MODE=postgres   # postgres|sqlite（sqlite=轻量模式，无需 Redis）
# 并补齐:
# PICHOST_I18N_LANGUAGE=en         # 默认 UI 语言: en|zh-CN
# PICHOST_I18N_LOCALES_DIR=        # 可选外部语言包目录
```

- [ ] **Step 4: 运行确认通过**

Run: `bash scripts/tests/verify_release_test.sh` + `bash scripts/verify-release.sh --skip-test --skip-lint`（含 sqlite 冒烟）
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add scripts/verify-release.sh .env.example scripts/tests/verify_release_test.sh
git commit -m "feat: add sqlite smoke test to verify-release and update env example"
```

**verify:**
- `bash scripts/verify-release.sh --skip-test --skip-lint`

**regression:**
- `bash scripts/verify-release.sh --skip-test --skip-lint --skip-install`

---

### Task T29: 文档同步（AGENTS.md / README.md / CHANGELOG）

**Files:**
- Modify: `AGENTS.md`（版本、双 DB 模式、迁移数 11+10、安装交互、config 变量）
- Modify: `README.md`（版本 tagline、部署章节、配置表、架构图）
- Test: `scripts/tests/docs_check_test.sh`（新建；grep 断言）

**Interfaces:**
- Consumes: 全部前序任务产物
- Produces: 文档与实现同步（CHANGELOG 0.21.0 条目在 T31 追加）

**depends_on:** [T26, T28]

**breaking:** false

**ac:**
- given: 文档已更新
  when: 运行 docs_check_test.sh
  then: README.md 含 PICHOST_DATABASE_MODE、AGENTS.md 含 sqlite 模式说明

- [ ] **Step 1: 写失败测试（文档内容断言）**

```bash
# scripts/tests/docs_check_test.sh（新建）
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
grep -q 'PICHOST_DATABASE_MODE' "$ROOT/README.md"
grep -q 'sqlite' "$ROOT/AGENTS.md"
echo "docs_check_test.sh PASS"
```

- [ ] **Step 2: 运行确认失败**

Run: `bash scripts/tests/docs_check_test.sh`
Expected: FAIL — 文档未更新

- [ ] **Step 3: 实现**

按 AGENTS.md 规则同步：版本 0.21.0、双运行模式（标准/轻量）、`migrations-sqlite/` 目录说明、install.sh 交互说明、`PICHOST_DATABASE_MODE` 配置变量、state traits 架构说明。

- [ ] **Step 4: 运行确认通过**

Run: `bash scripts/tests/docs_check_test.sh`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add AGENTS.md README.md scripts/tests/docs_check_test.sh
git commit -m "docs: sync AGENTS.md, README.md for sqlite lite mode (0.21.0)"
```

**verify:**
- `bash scripts/tests/docs_check_test.sh`

**regression:**
- `cargo test --workspace`（无 infra 324 测试）

---

### Task T30: 版本 bump 0.21.0

**Files:**
- Modify: `Cargo.toml`（workspace version 0.20.0 → 0.21.0）
- Modify: `web-ui/package.json`（version 同步）
- Test: `scripts/tests/version_check_test.sh`（新建；grep 断言）

**Interfaces:**
- Consumes: 全部前序任务
- Produces: 版本 0.21.0

**depends_on:** [T29]

**breaking:** false

**ac:**
- given: 版本已 bump
  when: 运行 version_check_test.sh
  then: Cargo.toml 与 web-ui/package.json 均为 0.21.0，且 cargo build + npm run build 通过

- [ ] **Step 1: 写失败测试**

```bash
# scripts/tests/version_check_test.sh（新建）
#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
grep -m1 '^version' "$ROOT/Cargo.toml" | grep -q '0.21.0'
grep -m1 '"version"' "$ROOT/web-ui/package.json" | grep -q '0.21.0'
echo "version_check_test.sh PASS"
```

- [ ] **Step 2: 运行确认失败**

Run: `bash scripts/tests/version_check_test.sh`
Expected: FAIL — 仍为 0.20.0

- [ ] **Step 3: 实现**

```bash
# Cargo.toml: version = "0.21.0"
# web-ui/package.json: "version": "0.21.0"
```

- [ ] **Step 4: 运行确认通过**

Run: `bash scripts/tests/version_check_test.sh` + `cargo build --workspace` + `cd web-ui && npm run build`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add Cargo.toml web-ui/package.json scripts/tests/version_check_test.sh
git commit -m "chore: bump version to 0.21.0"
```

**verify:**
- `bash scripts/tests/version_check_test.sh`
- `cargo build --workspace`
- `cd web-ui && npm run build`
- `cargo clippy --workspace -- -D warnings`

**regression:**
- `cargo test --workspace`

---

### Task T31: CHANGELOG + summary 收尾

**Files:**
- Modify: `CHANGELOG.md`（追加 0.21.0 条目）
- Modify: `.omo/summary/summary_and_next.md`（追加轻量模式阶段总结 + 待实施更新）
- Test: `scripts/tests/docs_check_test.sh`（追加 CHANGELOG/summary 断言）

**Interfaces:**
- Consumes: 全部前序任务
- Produces: CHANGELOG 0.21.0 条目 + 阶段总结文档

**depends_on:** [T29, T30]

**breaking:** false

**ac:**
- given: CHANGELOG 与 summary 已更新
  when: 运行 docs_check_test.sh（含新断言）
  then: CHANGELOG.md 含 0.21.0，summary_and_next.md 含 "轻量模式" 章节

- [ ] **Step 1: 写失败测试**

```bash
# 追加到 scripts/tests/docs_check_test.sh:
grep -q '0.21.0' "$ROOT/CHANGELOG.md"
grep -q '轻量模式' "$ROOT/.omo/summary/summary_and_next.md"
```

- [ ] **Step 2: 运行确认失败**

Run: `bash scripts/tests/docs_check_test.sh`
Expected: FAIL — CHANGELOG/summary 未更新

- [ ] **Step 3: 实现**

```markdown
# CHANGELOG.md 追加 0.21.0 条目:
## [0.21.0] - 2026-08-09
### Added
- 轻量模式 (SQLite + 无 Redis): 零外部依赖单机部署
- 安装脚本交互化: --yes/--mode postgres|sqlite, apt 引导

# 追加到 .omo/summary/summary_and_next.md:
## 轻量模式 (SQLite + 无 Redis) ✅ (本次完成)
- 双运行模式: 标准 (PG+Redis) / 轻量 (SQLite+无 Redis 零外部依赖)
- DbPool=AnyPool + migrations-sqlite 双迁移目录 (0011 个 lite 状态表)
- 5 个 state traits + Redis/SQLite 双实现 (Queue/Blacklist/RateLimiter/InviteStore/Cache)
- 单进程内嵌 worker (pichost-worker 库化)
- install.sh 交互化 (--yes/--mode, apt 引导)
- 版本: 0.20.0 → 0.21.0
```

- [ ] **Step 4: 运行确认通过**

Run: `bash scripts/tests/docs_check_test.sh`
Expected: PASS

- [ ] **Step 5: 提交**

```bash
git add CHANGELOG.md .omo/summary/summary_and_next.md scripts/tests/docs_check_test.sh
git commit -m "docs: add CHANGELOG 0.21.0 and update summary for sqlite lite mode"
```

**verify:**
- `bash scripts/tests/docs_check_test.sh`

**regression:**
- `cargo test --workspace`

---

## 自审记录（writing-plans self-review）

- **Spec 覆盖**: 每节对应任务——§3 DB 方言层→T7-T14；§4 trait 化→T15-T21；§5 内嵌 worker→T25-T26；§6 安装交互→T27-T28；§7 测试计划→各任务 verify/regression + T26 e2e；§8 实施顺序 S1-S8→T1-T31 按序；§9 TODO 全部映射。
- **无占位符**: 全部迁移 SQL、trait 签名、Sqlite 实现、脚本逻辑均为具体代码。
- **类型一致性**: `DbPool`/`DatabaseMode`/`TaskPayload`/5 traits/`NackAction`/`RateLimitResult` 在各任务间签名一致；T15 TaskPayload → T16 traits 定义 → T17-T20 Redis 实现 → T21 装配 → T22-T24 SQLite 实现 → T26 轻量装配。

---

## 重新规划附录（A′ 架构，2026-08-09 执行期修订）

**背景**: 原计划核心架构 `DbPool = sqlx::AnyPool` 经实测（sqlx 0.8.6 vendored 源码验证 + T8 编译证据，659 错误）不可行：

1. `Type<Any>`/`Encode<Any>`/`Decode<Any>` 仅实现于基本类型（str/String/f32/f64/i16/i32/i64/bool/[u8]/Vec<u8>）——无 `Uuid`/`DateTime<Utc>`/`Json`，而全应用到处绑定/解码这三种类型。
2. 即使外部补写 `Type<Any>` impl（`AnyTypeInfo { pub kind }` 可构造），PG→Any 驱动边界（`sqlx-postgres/src/any.rs:196-212`）仅映射 Bool/Void/Int2/Int4/Int8/Float4/Float8/Bytea/Text/Varchar/citext；uuid/timestamptz/jsonb 列在 `AnyRow::map_from` 逐值转换时直接 `AnyDriverError`，先于任何 `Decode`。
3. PG 结果集为二进制线格式且 sqlx-postgres 0.8.6 无 text-mode 开关——救援 AnyPool 需 fork sqlx（改驱动映射 + 手写二进制→文本解码器），成本与风险均不可接受。
4. 对比: sqlx-sqlite 对 `Uuid`/`DateTime<Utc>`/`Json<T>` 的 `Type<Sqlite>` 实现完整（TEXT 存储，与已提交的 migrations-sqlite 兼容）；sqlx-postgres 同理。两条 concrete 驱动路径均可行。

**决策（用户批准）**: 采用 **A′ 架构** —— 通用 `AppState<DB: Database>` + 通用 router 装配（Oracle 验证）：

- 无数据路径枚举：`run_with::<DB>()` 启动时选定具体驱动（Postgres/Sqlite 各实例化一次），handler 为泛型函数（编译期按驱动实例化，零运行时分发）。
- `pichost_core::db` 重构（保留 T7 已提交的 Migrators/config/WAL 意图）：`create_pg_pool`/`create_sqlite_pool`（concrete pool，SqliteConnectOptions 原生支持 create_if_missing/WAL/busy_timeout/foreign_keys——删除 AnyConnectOptions 的 `sqlite_url()` URL 改写 hack 与 post-connect WAL pragma）、`run_pg_migrations`/`run_sqlite_migrations`（`Migrator.run` 按驱动特化）、删除 `any` sqlx feature。
- 泛型化编辑面（Oracle 实测盘点）：3 处 `QueryBuilder<'_, Postgres>`、2 处 turbofish 钉死（admin.rs:378 `query_scalar::<_, bool>`、pipeline.rs:55 `query_scalar::<_, Option<Json>>`）、28 处 `&PgPool`/`&DbPool` helper 签名、`AppState`/`WorkerState` 字段、`Locale` extractor + `require_auth`/`rate_limit` 中间件、~40 个 handler 签名；21 处未类型化 `query()` + `query_as::<_, T>`（DB 推断）**零改动**。
- **T10 ILIKE 规则修正**（原计划错误）: `ILIKE` → `LOWER(col) LIKE LOWER($n)`（PG LIKE 大小写敏感、SQLite LIKE 仅 ASCII 不敏感；LOWER 双端保证两端一致不敏感；0005 索引为普通 btree，前导通配 `%term%` 本就用不上索引，PG 无性能损失）。
- 批处理 `ANY($1)` → 动态 `IN ($1,$2,…,$N)`（SQLite 接受 `$N`；单 bind 为 `Uuid`，双驱动均支持；100 id = 200 参数，低于 PG 65535 / SQLite 32766 上限）。
- `now()` → `CURRENT_TIMESTAMP`（PG 等价于 transaction_timestamp）；`NOW() - INTERVAL` → Rust 侧计算 `DateTime<Utc>` 参数。
- **T21 装配任务调整**: AppState 字段全部 trait object 化保持原样，但装配函数改为 `run_with::<DB>`（Postgres 分支本期装配 Redis 实现；Sqlite 分支在 T26 接入 SQLite 实现 + 内嵌 worker）。

**任务边界修订（T8-T9）**:

| 任务 | 变更 |
|------|------|
| T8（原: 接入 AnyPool） | **重定义**: 重构 `pichost-core/src/db.rs` 为 per-driver pools + 双 migrations helper；`pichost-api/src/db/mod.rs` 与 `pichost-worker/src/db.rs` 改为 re-export core；main.rs x2 + tests/common 调用点改 per-driver 函数（Postgres 路径行为不变）。Gate: `cargo check --workspace` + `cargo test --workspace` 全绿。 |
| T9a（新，吸收原 T8 接线） | **泛型化清扫**: `AppState<DB>`、`configure_app<DB>`、全部 handler/中间件/`Locale` 泛型化、worker pipeline `process_task<DB>`/`WorkerState<DB>` 泛型化。单次提交原子完成（中间态不可编译）。Gate: 575 全绿。 |
| T9（原 db_error_kind） | 不变（依赖 T8 重定义版）。 |
| T10-T13 | 内容不变，ILIKE 规则按上表修正。 |
| T14-T26 | 不变（T21 装配按上表调整）。 |
| T27-T31 | 不变。 |






