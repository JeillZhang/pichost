# PicHost 首次安装终端初始化向导 — 实现计划

> **日期**: 2026-08-15
> **来源规格**: `docs/superpowers/specs/2026-08-15-pichost-first-run-wizard-design.md`
> **目标**: 服务首次启动(用户数 == 0)且 stdin 为 TTY 时,自动运行终端向导:① 配置(JWT/公开 URL/语言)→ 写 `.env` → 进程内生效 ② 创建首个管理员;`--setup` 强制;无 TTY 跳过 + WARN(强制则报错);单进程两阶段,无需重启
> **版本**: 0.23.0 → 0.24.0(feature)

## Agent Worker Instructions

- **必读上下文**: 规格文档 `docs/superpowers/specs/2026-08-15-pichost-first-run-wizard-design.md`、`AGENTS.md`(crate 边界 / 函数 ≤50 行 / 行 ≤120 字符 / 测试约定)、`.omo/summary/summary_and_next.md`
- **推荐执行模式**: `superpowers:subagent-driven-development`(每个任务一个 fresh subagent + 任务间评审);备选 `executing-plans`
- **强制验证**: 每任务收尾 `cargo clippy --workspace -- -D warnings`;最终 `cargo test --workspace` + `cargo test --workspace -- --include-ignored`(有 Docker 时)+ `cd web-ui && npm run build`
- **版本提醒**: 代码任务全部完成后执行 T8(workspace Cargo.toml + web-ui/package.json + CHANGELOG 对齐 0.24.0),随后 T9 文档同步(README/AGENTS/summary)并以 `docs: auto-sync ...` 单提交(AGENTS.md 强制)
- **提交风格**: 语义化英文 commit(`feat:` / `refactor:` / `test:` / `docs:` / `chore:`);每个任务独立提交
- **TDD 顺序**: 每个任务先写 test_code → 验证失败(红)→ 写 impl_code → 验证通过(绿)→ commit

**Goal**: First-run terminal wizard that configures JWT/public URL/language and creates the first admin on initial startup, with non-TTY environments skipping gracefully.

**Architecture**: A new `pichost-api/src/setup/` module (prompts trait + env writer + orchestration + admin creation) hooks into both startup branches (main.rs PG branch, lib.rs sqlite branch) after migrations. Config writes go to `.env` (probe: `PICHOST_ENV_FILE` → `/etc/pichost/.env` → CWD `.env`) with `std::env::set_var` sync so the running process picks up new values without restart. User-creation helpers are extracted from `routes/auth.rs`/`routes/users.rs` into `services/user_ops.rs` (DRY). Prompt abstraction (`Prompt` trait + `DialoguerPrompts` + `MockPrompts`) makes wizard logic testable without a TTY.

**Tech Stack**: Rust 2021, axum 0.8, sqlx 0.8 (runtime queries only), figment config, dialoguer 0.12 (new), std `io::IsTerminal` (no new dep), argon2 (existing), rand (existing).

## Global Constraints

- Rust 函数 ≤50 行,行 ≤120 字符(AGENTS.md)
- `cargo clippy --workspace -- -D warnings` 零警告(AGENTS.md 强制)
- sqlx 仅运行时查询,禁止 `query!` 宏;迁移自动应用(`run_pg_migrations` / `run_sqlite_migrations`)
- 双驱动泛型:函数必须携带 `DbType` + `DbRow` + `for<'q> Encode` 等 where 子句(见 `pichost-core/src/db.rs` 既有模式)
- 配置 env 前缀 `PICHOST_`;嵌套键双下划线(`PICHOST_AUTH__JWT_SECRET`);JWT 校验规则 ≥32 字符
- 不写 config.toml(env 优先且 CWD 相对路径);不碰 pichost-core 依赖、web-ui、安装脚本、DB 迁移
- i18n 双语言键集必须保持相等(en/zh-CN `messages.toml` 同步新增,现有键集相等性测试把关)
- 集成测试约定:`#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`;SQLite 内存池模式参考 `tests/sqlite_smoke_test.rs`
- `--setup` 强制 + 无 TTY → 报错退出;自动触发 + 无 TTY → WARN 后跳过;两者均不影响服务正常启动

---

### Task T0: Add dialoguer dependency and `--setup` CLI flag

**Breaking:** false(新增依赖与 CLI 变体,既有命令行为不变)

**Files:**
- Modify: `pichost-api/Cargo.toml` ([dependencies] 新增一行)
- Modify: `pichost-api/src/cli.rs` (enum + parse + USAGE)
- Test: `pichost-api/tests/cli_test.rs` (追加 1 个断言)

**Interfaces:**
- Produces: `CliCommand::Setup` 变体;`parse_cli_args(&["--setup"]) == Ok(CliCommand::Setup)`;USAGE 含 `--setup`;`pichost-api` 依赖 `dialoguer = "0.12"`(default features 含 password/zeroize)

**Acceptance Criteria:**
- given: 编译后的 crate 依赖含 dialoguer 0.12
  when: 运行 `cargo tree -p pichost-api -i dialoguer`
  then: 输出显示 dialoguer 0.12 作为 pichost-api 直接依赖
- given: `parse_cli_args` 收到 `["--setup"]`
  when: 调用解析
  then: 返回 `Ok(CliCommand::Setup)`
- given: USAGE 常量
  when: 检查内容
  then: 包含 `--setup`

**Regression:**
- `cargo test -p pichost-api --test cli_test`(既有断言必须保持通过:Run/InstallService/UninstallService/Service/Help/未知参数)

- [ ] **Step 1: Write the failing test**

`pichost-api/tests/cli_test.rs` 追加:

```rust
#[test]
fn parses_setup_flag() {
    let args: Vec<String> = vec!["--setup".into()];
    assert_eq!(parse_cli_args(&args), Ok(CliCommand::Setup));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pichost-api --test cli_test`
Expected: FAIL — `CliCommand::Setup` 不存在(编译错误)

- [ ] **Step 3: Write minimal implementation**

`pichost-api/Cargo.toml` `[dependencies]` 新增:

```toml
dialoguer = "0.12"
```

`pichost-api/src/cli.rs`:

```rust
pub enum CliCommand {
    Run,
    Setup,
    InstallService,
    UninstallService,
    Service,
    Help,
}

pub const USAGE: &str =
    "Usage: pichost-api [--setup|--install-service|--uninstall-service|--service]";
```

`parse_cli_args` 的 `[flag]` match 追加一行(位于 `--install-service` 之前):

```rust
"--setup" => Ok(CliCommand::Setup),
```

注意:`service.rs dispatch_cli` 已有 `_ => unreachable!("Run/Help handled in main")` catch-all(L78),新增变体无需修改。

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pichost-api --test cli_test && cargo clippy --workspace -- -D warnings`
Expected: 全 PASS,clippy 零警告

- [ ] **Step 5: Commit**

```bash
git add pichost-api/Cargo.toml pichost-api/src/cli.rs pichost-api/tests/cli_test.rs
git commit -m "feat: add --setup cli flag and dialoguer dependency"
```

**Verify:**
- `cargo test -p pichost-api --test cli_test`
- `cargo clippy --workspace -- -D warnings`

---

### Task T1: Create `services/user_ops.rs` helper module

**Breaking:** false(纯新增模块,无消费方改动)

**Files:**
- Create: `pichost-api/src/services/user_ops.rs`(含 `#[cfg(test)]` 单元测试)
- Modify: `pichost-api/src/services/mod.rs`(声明 `pub mod user_ops;`)

**Interfaces:**
- Produces(供 T2/T6 消费):
  - `pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error>`
  - `pub async fn count_users<DB: DbType>(pool: &Pool<DB>) -> Result<i64, sqlx::Error>`(where 子句同 auth.rs `count_existing_users` 现状)
  - `pub async fn insert_user<DB: DbType>(pool: &Pool<DB>, username: &str, email: &Option<String>, hash: &str, is_admin: bool, storage_quota: Option<i64>) -> Result<Uuid, sqlx::Error>`(where 子句同 auth.rs `insert_user` 现状;唯一冲突经 `db_error_kind` 判定)
- Consumes: 无(纯提取)

**Acceptance Criteria:**
- given: `hash_password("secret123")`
  when: 调用
  then: 返回以 `$argon2id$` 开头的字符串,且可用同密码 `verify_password` 通过、错密码失败
- given: 新模块编译
  when: `cargo check -p pichost-api`
  then: 零错误(泛型函数在 PG/SQLite 双驱动下均可实例化)
- given: `cargo test -p pichost-api user_ops`
  when: 运行
  then: 单元测试全 PASS

**Regression:**
- `cargo test -p pichost-api`(crate 编译与既有测试零回归)

- [ ] **Step 1: Write the failing test**

`pichost-api/src/services/user_ops.rs` 内(模块创建时同时写入):

```rust
#[cfg(test)]
mod tests {
    use super::hash_password;
    use argon2::password_hash::PasswordHash;
    use argon2::{Argon2, PasswordVerifier};

    #[test]
    fn hash_password_produces_verifiable_argon2_hash() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(hash.starts_with("$argon2id$"), "hash: {hash}");
        let parsed = PasswordHash::new(&hash).unwrap();
        assert!(Argon2::default()
            .verify_password(b"correct horse battery staple", &parsed)
            .is_ok());
        assert!(Argon2::default()
            .verify_password(b"wrong password", &parsed)
            .is_err());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pichost-api user_ops`
Expected: FAIL — 模块/函数不存在

- [ ] **Step 3: Write minimal implementation**

`pichost-api/src/services/mod.rs` 追加:

```rust
pub mod user_ops;
```

`pichost-api/src/services/user_ops.rs`(每个函数 ≤50 行):

```rust
use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHasher};
use pichost_core::DbType;
use sqlx::Pool;
use uuid::Uuid;

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
}

pub async fn count_users<DB: DbType>(pool: &Pool<DB>) -> Result<i64, sqlx::Error>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (i64,): crate::db::DbRow<DB>,
{
    sqlx::query_scalar("SELECT COUNT(*) FROM users").fetch_one(pool).await
}

pub async fn insert_user<DB: DbType>(
    pool: &Pool<DB>,
    username: &str,
    email: &Option<String>,
    hash: &str,
    is_admin: bool,
    storage_quota: Option<i64>,
) -> Result<Uuid, sqlx::Error>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    sqlx::query_scalar(
        "INSERT INTO users (username, email, password_hash, is_admin, storage_quota) \
         VALUES ($1, $2, $3, $4, $5) RETURNING id",
    )
    .bind(username)
    .bind(email)
    .bind(hash)
    .bind(is_admin)
    .bind(storage_quota)
    .fetch_one(pool)
    .await
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pichost-api user_ops && cargo clippy --workspace -- -D warnings`
Expected: 1 PASS,clippy 零警告

- [ ] **Step 5: Commit**

```bash
git add pichost-api/src/services/user_ops.rs pichost-api/src/services/mod.rs
git commit -m "refactor: add shared user creation helpers in services/user_ops"
```

**Verify:**
- `cargo test -p pichost-api user_ops -- --exact`
- `cargo clippy --workspace -- -D warnings`

---

### Task T2: Rewire auth and users routes to use `user_ops` helpers

**Breaking:** false(行为保持重构;路由签名与错误信封逐字节不变)

**Files:**
- Modify: `pichost-api/src/routes/auth.rs`(3 个私有助手改为薄包装)
- Modify: `pichost-api/src/routes/users.rs`(`hash_new_password` 改为薄包装)

**Interfaces:**
- Consumes: T1(`user_ops::{hash_password, count_users, insert_user}`)
- Produces: 无新公共接口(行为与现状完全一致)

**Acceptance Criteria:**
- given: 既有注册集成测试(auth_test)
  when: 提取后全量运行
  then: 全部通过(注册/邀请码/首个用户 auto-admin 行为零变化)
- given: 既有改密集成测试(users_test)
  when: 运行
  then: 全部通过(密码规则/校验行为零变化)
- given: `insert_user` 包装层错误映射
  when: 触发 UniqueViolation
  then: 仍返回 409 + `auth.username_exists`(由 auth_test 重复用户名用例覆盖)

**Regression:**
- `cargo test -p pichost-api --test auth_test`
- `cargo test -p pichost-api --test users_test`

- [ ] **Step 1: 运行既有回归建立绿基线(无新测试——行为必须逐字节保持,以既有测试为规格)**

Run: `cargo test -p pichost-api --test auth_test && cargo test -p pichost-api --test users_test`
Expected: 全 PASS(若此处 FAIL,先修复既有问题再继续)

- [ ] **Step 2: 实现薄包装(实现即本任务的"impl_code";第 1 步绿基线即其规格)**

`pichost-api/src/routes/auth.rs` 三个私有助手替换为薄包装(签名不变,错误信封映射留在原层):

```rust
fn hash_password(
    password: &str,
    locale: Language,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    crate::services::user_ops::hash_password(password).map_err(|e| {
        tracing::warn!("Password hashing failed: {e}");
        error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
    })
}
```

```rust
async fn count_existing_users<DB: DbType>(
    pool: &Pool<DB>,
    locale: Language,
) -> Result<i64, (StatusCode, Json<serde_json::Value>)>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    (i64,): crate::db::DbRow<DB>,
{
    crate::services::user_ops::count_users(pool).await.map_err(|e| {
        tracing::warn!("User count query failed: {e}");
        error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
    })
}
```

```rust
async fn insert_user<DB: DbType>(
    state: &AppState<DB>,
    username: &str,
    email: &Option<String>,
    hash: &str,
    is_admin: bool,
    storage_quota: Option<i64>,
    locale: Language,
) -> Result<Uuid, (StatusCode, Json<serde_json::Value>)>
where
    /* 保持原 where 子句不变(逐字保留,见 auth.rs L246-257) */
{
    crate::services::user_ops::insert_user(
        &state.pool,
        username,
        email,
        hash,
        is_admin,
        storage_quota,
    )
    .await
    .map_err(|e| {
        if pichost_core::db::db_error_kind(&e) == pichost_core::db::DbErrorKind::UniqueViolation {
            return error_json(locale, StatusCode::CONFLICT, "auth.username_exists");
        }
        tracing::warn!("User registration db error: {e}");
        error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
    })
}
```

`pichost-api/src/routes/users.rs` `hash_new_password`(L527-543)替换为:

```rust
fn hash_new_password(
    new_password: &str,
    locale: Language,
) -> Result<String, (StatusCode, Json<serde_json::Value>)> {
    crate::services::user_ops::hash_password(new_password).map_err(|e| {
        tracing::warn!("Password hashing failed: {e}");
        error_json(locale, StatusCode::INTERNAL_SERVER_ERROR, "common.internal_error")
    })
}
```

删除 users.rs 不再使用的 `SaltString`/`PasswordHasher`/`OsRng` import(保留 `PasswordHash`/`Argon2`/`PasswordVerifier` 供 `verify_current_password` 使用;该组 import 位于同一行,按需裁剪)。

- [ ] **Step 3: Run tests to verify they pass**

Run: `cargo test -p pichost-api --test auth_test && cargo test -p pichost-api --test users_test && cargo test -p pichost-api user_ops && cargo clippy --workspace -- -D warnings`
Expected: 全 PASS(198 路由测试零回归),clippy 零警告

- [ ] **Step 4: Commit**

```bash
git add pichost-api/src/routes/auth.rs pichost-api/src/routes/users.rs
git commit -m "refactor: rewire auth and users routes to shared user_ops helpers"
```

**Verify:**
- `cargo test -p pichost-api --test auth_test`
- `cargo test -p pichost-api --test users_test`
- `cargo test -p pichost-api user_ops -- --exact`
- `cargo clippy --workspace -- -D warnings`

---

### Task T3: Add `Prompt` trait with dialoguer and mock implementations

**Breaking:** false(纯新增模块;lib.rs 仅追加模块声明)

**Files:**
- Create: `pichost-api/src/setup/mod.rs`(仅声明 `pub mod prompts;`)
- Create: `pichost-api/src/setup/prompts.rs`(含 `#[cfg(test)]` 单元测试)
- Modify: `pichost-api/src/lib.rs`(声明 `pub mod setup;`)

**Interfaces:**
- Produces(供 T6/T7 消费,均为 `pub` 以便 tests/ 外部集成测试使用):
  - `pub trait Prompt` — `select(&mut self, prompt, items: &[&str], default: usize) -> Result<usize, Box<dyn Error + Send + Sync>>`、`input(&mut self, prompt, default: Option<&str>) -> Result<String, Box<dyn Error + Send + Sync>>`、`password(&mut self, prompt, confirm_prompt: Option<&str>) -> Result<String, Box<dyn Error + Send + Sync>>`、`confirm(&mut self, prompt, default: bool) -> Result<bool, Box<dyn Error + Send + Sync>>`
  - `pub struct DialoguerPrompts`(impl Prompt,委托 dialoguer 0.12)
  - `pub enum MockReply { Select(usize), Input(String), Password(String), Confirm(bool) }`
  - `pub struct MockPrompts { queue: VecDeque<MockReply> }` + `pub fn new(replies: Vec<MockReply>) -> Self`(impl Prompt;队列耗尽返回 Err,模拟 EOF/取消)
- Consumes: T0(dialoguer 依赖)

**Acceptance Criteria:**
- given: `MockPrompts::new(vec![Select(1), Input("x"), Password("p"), Confirm(true)])`
  when: 依次调用 select/input/password/confirm
  then: 依序返回 1 / "x" / "p" / true
- given: `MockPrompts` 队列为空
  when: 调用任意 prompt 方法
  then: 返回 Err(模拟 EOF 取消,调用方不 panic)
- given: `DialoguerPrompts`
  when: 编译并检查 4 个方法签名
  then: 与 trait 声明一致(非 TTY 环境不实际交互)

**Regression:**
- `cargo test -p pichost-api`(crate 编译与既有测试零回归)

- [ ] **Step 1: Write the failing test**

`pichost-api/src/setup/prompts.rs` 内(模块创建时同时写入):

```rust
#[cfg(test)]
mod tests {
    use super::{MockPrompts, MockReply, Prompt};

    #[test]
    fn mock_prompts_reply_in_order() {
        let mut p = MockPrompts::new(vec![
            MockReply::Select(1),
            MockReply::Input("https://img.example.com".into()),
            MockReply::Password("secret".into()),
            MockReply::Confirm(true),
        ]);
        assert_eq!(p.select("lang", &["en", "zh-CN"], 0).unwrap(), 1);
        assert_eq!(p.input("url", None).unwrap(), "https://img.example.com");
        assert_eq!(p.password("pw", None).unwrap(), "secret");
        assert!(p.confirm("admin?", true).unwrap());
    }

    #[test]
    fn mock_prompts_exhausted_reply_errors() {
        let mut p = MockPrompts::new(vec![]);
        let err = p.confirm("any?", false).unwrap_err();
        assert!(!err.to_string().is_empty());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pichost-api setup::prompts`
Expected: FAIL — 模块/类型不存在

- [ ] **Step 3: Write minimal implementation**

`pichost-api/src/lib.rs` 模块声明追加:

```rust
pub mod setup;
```

`pichost-api/src/setup/mod.rs`:

```rust
pub mod prompts;
```

`pichost-api/src/setup/prompts.rs`(每个函数 ≤50 行):

```rust
use std::collections::VecDeque;
use std::error::Error;

pub trait Prompt {
    fn select(
        &mut self,
        prompt: &str,
        items: &[&str],
        default: usize,
    ) -> Result<usize, Box<dyn Error + Send + Sync>>;
    fn input(
        &mut self,
        prompt: &str,
        default: Option<&str>,
    ) -> Result<String, Box<dyn Error + Send + Sync>>;
    fn password(
        &mut self,
        prompt: &str,
        confirm_prompt: Option<&str>,
    ) -> Result<String, Box<dyn Error + Send + Sync>>;
    fn confirm(
        &mut self,
        prompt: &str,
        default: bool,
    ) -> Result<bool, Box<dyn Error + Send + Sync>>;
}

pub struct DialoguerPrompts;

impl Prompt for DialoguerPrompts {
    fn select(
        &mut self,
        prompt: &str,
        items: &[&str],
        default: usize,
    ) -> Result<usize, Box<dyn Error + Send + Sync>> {
        use dialoguer::Select;
        Ok(Select::new()
            .with_prompt(prompt)
            .items(items)
            .default(default)
            .interact()?)
    }

    fn input(
        &mut self,
        prompt: &str,
        default: Option<&str>,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        use dialoguer::Input;
        let mut input = Input::<String>::new().with_prompt(prompt);
        if let Some(d) = default {
            input = input.default(d.to_string());
        }
        Ok(input.interact_text()?)
    }

    fn password(
        &mut self,
        prompt: &str,
        confirm_prompt: Option<&str>,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        use dialoguer::Password;
        let mut password = Password::new().with_prompt(prompt);
        if let Some(cp) = confirm_prompt {
            password = password.with_confirmation(cp, "mismatch");
        }
        Ok(password.interact()?)
    }

    fn confirm(
        &mut self,
        prompt: &str,
        default: bool,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        use dialoguer::Confirm;
        Ok(Confirm::new().with_prompt(prompt).default(default).interact()?)
    }
}

#[derive(Debug)]
pub enum MockReply {
    Select(usize),
    Input(String),
    Password(String),
    Confirm(bool),
}

pub struct MockPrompts {
    queue: VecDeque<MockReply>,
}

impl MockPrompts {
    pub fn new(replies: Vec<MockReply>) -> Self {
        Self { queue: replies.into() }
    }
}

impl Prompt for MockPrompts {
    fn select(
        &mut self,
        _prompt: &str,
        _items: &[&str],
        _default: usize,
    ) -> Result<usize, Box<dyn Error + Send + Sync>> {
        match self.queue.pop_front() {
            Some(MockReply::Select(i)) => Ok(i),
            _ => Err("mock prompt queue exhausted".into()),
        }
    }

    fn input(
        &mut self,
        _prompt: &str,
        _default: Option<&str>,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        match self.queue.pop_front() {
            Some(MockReply::Input(s)) => Ok(s),
            _ => Err("mock prompt queue exhausted".into()),
        }
    }

    fn password(
        &mut self,
        _prompt: &str,
        _confirm_prompt: Option<&str>,
    ) -> Result<String, Box<dyn Error + Send + Sync>> {
        match self.queue.pop_front() {
            Some(MockReply::Password(s)) => Ok(s),
            _ => Err("mock prompt queue exhausted".into()),
        }
    }

    fn confirm(
        &mut self,
        _prompt: &str,
        _default: bool,
    ) -> Result<bool, Box<dyn Error + Send + Sync>> {
        match self.queue.pop_front() {
            Some(MockReply::Confirm(b)) => Ok(b),
            _ => Err("mock prompt queue exhausted".into()),
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pichost-api setup::prompts && cargo clippy --workspace -- -D warnings`
Expected: 2 PASS,clippy 零警告

- [ ] **Step 5: Commit**

```bash
git add pichost-api/src/lib.rs pichost-api/src/setup/mod.rs pichost-api/src/setup/prompts.rs
git commit -m "feat: add Prompt trait with dialoguer and mock implementations"
```

**Verify:**
- `cargo test -p pichost-api setup::prompts`
- `cargo clippy --workspace -- -D warnings`

---

### Task T4: Add `EnvWriter` for idempotent `.env` updates

**Breaking:** false(纯新增模块)

**Files:**
- Modify: `pichost-api/src/setup/mod.rs`(声明 `pub mod env_writer;`)
- Create: `pichost-api/src/setup/env_writer.rs`
- Test: `pichost-api/tests/env_writer_test.rs`

**Interfaces:**
- Produces(供 T6 消费,`pub` 供外部测试):
  - `pub fn upsert_env(content: &str, updates: &[(&str, &str)]) -> String` — 删除目标键的单/双下划线变体行,保留其他行,末尾追加规范形式
  - `pub fn probe_env_path(explicit: Option<&Path>, system_dir: &Path, cwd: &Path) -> Option<PathBuf>` — explicit 优先(即使不存在也返回);其次 `system_dir/.env` 存在;其次 `cwd/.env` 存在;否则 None
  - `pub fn validate_jwt_secret(secret: &str) -> bool` — `len() >= 32`
  - `pub fn validate_public_url(url: &str) -> bool` — `url::Url::parse` 成功且 scheme ∈ {http, https}
  - `pub fn generate_jwt_secret() -> String` — `rand::rngs::OsRng` 32 字节 hex
  - `pub fn apply_env_file(path: &Path, updates: &[(&str, &str)]) -> std::io::Result<()>` — 读(缺则空)→ upsert → 写临时文件 → chmod 600(仅新建,unix)→ `rename` 原子替换
- Consumes: 无(rand/url 均为既有依赖)

**Acceptance Criteria:**
- given: 内容含 `PICHOST_AUTH_JWT_SECRET=old`(单下划线)与注释行的 .env 文本
  when: `upsert_env` 更新 `[("PICHOST_AUTH__JWT_SECRET", "abcdef0123456789abcdef0123456789")]`
  then: 结果不含单下划线变体行、不含旧值行、含 `PICHOST_AUTH__JWT_SECRET=abcdef...`,其他行(注释等)保留
- given: `probe_env_path(Some(p), _, _)`
  when: 调用
  then: 返回 `Some(p)`(显式覆盖优先,不检查存在性)
- given: 无显式路径且 system_dir/cwd 均无 .env
  when: 调用
  then: 返回 `None`
- given: `apply_env_file` 指向新路径
  when: 写入后检查
  then: 文件存在、内容为 upsert 结果、unix 权限 600
- given: `validate_jwt_secret("12345678901234567890123456789012")`、`validate_public_url("https://x")`、`validate_public_url("ftp://x")`
  when: 调用
  then: true / true / false;`generate_jwt_secret().len() >= 64`(32 字节 hex)

**Regression:**
- `cargo test -p pichost-api`(crate 编译零回归)

- [ ] **Step 1: Write the failing test**

`pichost-api/tests/env_writer_test.rs`:

```rust
use pichost_api::setup::env_writer::{
    apply_env_file, generate_jwt_secret, probe_env_path, upsert_env, validate_jwt_secret,
    validate_public_url,
};
use std::path::Path;

#[test]
fn upsert_removes_both_underscore_variants_and_appends_canonical() {
    let content = "# comment\nPICHOST_AUTH_JWT_SECRET=old\nKEEP=yes\n";
    let out = upsert_env(
        content,
        &[("PICHOST_AUTH__JWT_SECRET", "abcdef0123456789abcdef0123456789")],
    );
    assert!(out.contains("# comment"));
    assert!(out.contains("KEEP=yes"));
    assert!(!out.contains("PICHOST_AUTH_JWT_SECRET=old"));
    assert!(!out.contains("PICHOST_AUTH_JWT_SECRET=abcdef"));
    assert!(out.contains("PICHOST_AUTH__JWT_SECRET=abcdef0123456789abcdef0123456789"));
}

#[test]
fn probe_env_path_prefers_explicit_override() {
    let p = probe_env_path(
        Some(Path::new("/tmp/x.env")),
        Path::new("/nonexistent-dir"),
        Path::new("/nonexistent-cwd"),
    );
    assert_eq!(p, Some(std::path::PathBuf::from("/tmp/x.env")));
}

#[test]
fn probe_env_path_returns_none_when_nothing_exists() {
    let p = probe_env_path(None, Path::new("/nonexistent-dir"), Path::new("/nonexistent-cwd"));
    assert!(p.is_none());
}

#[test]
fn apply_env_file_creates_atomic_file_with_600_perms() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join(".env");
    apply_env_file(
        &path,
        &[("PICHOST_AUTH__JWT_SECRET", "abcdef0123456789abcdef0123456789")],
    )
    .unwrap();
    let content = std::fs::read_to_string(&path).unwrap();
    assert!(content.contains("PICHOST_AUTH__JWT_SECRET=abcdef0123456789abcdef0123456789"));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

#[test]
fn validation_rules() {
    assert!(validate_jwt_secret("12345678901234567890123456789012"));
    assert!(!validate_jwt_secret("short"));
    assert!(validate_public_url("https://img.example.com"));
    assert!(!validate_public_url("ftp://img.example.com"));
    assert!(generate_jwt_secret().len() >= 64);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pichost-api --test env_writer_test`
Expected: FAIL — 模块/函数不存在(编译错误)

- [ ] **Step 3: Write minimal implementation**

`pichost-api/src/setup/mod.rs` 追加:

```rust
pub mod env_writer;
```

`pichost-api/src/setup/env_writer.rs`(每个函数 ≤50 行):

```rust
use rand::RngCore;
use std::io;
use std::path::{Path, PathBuf};

fn single_underscore(key: &str) -> String {
    key.replace("__", "_")
}

pub fn upsert_env(content: &str, updates: &[(&str, &str)]) -> String {
    let mut result = String::new();
    for line in content.lines() {
        let key = line.split('=').next().unwrap_or("");
        let replaced = updates
            .iter()
            .any(|(k, _)| key == *k || key == single_underscore(k));
        if !replaced {
            result.push_str(line);
            result.push('\n');
        }
    }
    for (k, v) in updates {
        result.push_str(&format!("{k}={v}\n"));
    }
    result
}

pub fn probe_env_path(
    explicit: Option<&Path>,
    system_dir: &Path,
    cwd: &Path,
) -> Option<PathBuf> {
    if let Some(p) = explicit {
        return Some(p.to_path_buf());
    }
    let system = system_dir.join(".env");
    if system.exists() {
        return Some(system);
    }
    let local = cwd.join(".env");
    if local.exists() {
        return Some(local);
    }
    None
}

pub fn validate_jwt_secret(secret: &str) -> bool {
    secret.len() >= 32
}

pub fn validate_public_url(url: &str) -> bool {
    match url::Url::parse(url) {
        Ok(u) => matches!(u.scheme(), "http" | "https"),
        Err(_) => false,
    }
}

pub fn generate_jwt_secret() -> String {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn apply_env_file(path: &Path, updates: &[(&str, &str)]) -> io::Result<()> {
    let content = if path.exists() {
        std::fs::read_to_string(path)?
    } else {
        String::new()
    };
    let new_content = upsert_env(&content, updates);
    // 显式 .env.tmp 临时名(避免 with_extension 对点文件的非预期拼接)
    let tmp = path.with_extension("tmp");
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&tmp, new_content)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))?;
    }
    std::fs::rename(&tmp, path)
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pichost-api --test env_writer_test && cargo clippy --workspace -- -D warnings`
Expected: 5 PASS,clippy 零警告

- [ ] **Step 5: Commit**

```bash
git add pichost-api/src/setup/mod.rs pichost-api/src/setup/env_writer.rs pichost-api/tests/env_writer_test.rs
git commit -m "feat: add EnvWriter for idempotent .env updates"
```

**Verify:**
- `cargo test -p pichost-api --test env_writer_test`
- `cargo clippy --workspace -- -D warnings`

---

### Task T5: Add `setup.*` i18n message keys (en + zh-CN)

**Breaking:** false(i18n 目录新增键,键集相等性测试把关)

**Files:**
- Modify: `pichost-core/src/i18n/locales/en/messages.toml`(追加 20 键)
- Modify: `pichost-core/src/i18n/locales/zh-CN/messages.toml`(追加同 20 键)
- Test: `pichost-core/src/i18n.rs`(追加 1 个键集断言测试)

**Interfaces:**
- Produces(供 T6 消费): `setup.welcome`、`setup.language`、`setup.jwt_generated`、`setup.jwt_configured`、`setup.public_url`、`setup.public_url_skip`、`setup.admin_confirm`、`setup.admin_skip`、`setup.username`、`setup.password`、`setup.password_confirm`、`setup.email`、`setup.invalid_username`、`setup.invalid_url`、`setup.invalid_email`、`setup.invalid_password`、`setup.username_taken`、`setup.complete`、`setup.warn_notty`、`setup.env_path`

**Acceptance Criteria:**
- given: 两语言文件
  when: 运行既有键集相等性测试 + 新断言
  then: 全部 setup.* 键在 en/zh-CN 中均存在且值非空
- given: `I18n::global().t(ZhCN, "setup.welcome")`
  when: 调用
  then: 返回中文文案(非键名本身)

**Regression:**
- `cargo test -p pichost-core i18n`(既有键集相等性 + 热加载等测试)

- [ ] **Step 1: Write the failing test**

`pichost-core/src/i18n.rs` 测试模块内追加:

```rust
#[test]
fn setup_keys_present_in_both_locales() {
    let en = parse_toml(EMBEDDED_EN);
    let zh = parse_toml(EMBEDDED_ZH);
    let keys = [
        "setup.welcome", "setup.language", "setup.jwt_generated", "setup.jwt_configured",
        "setup.public_url", "setup.public_url_skip", "setup.admin_confirm", "setup.admin_skip",
        "setup.username", "setup.password", "setup.password_confirm", "setup.email",
        "setup.invalid_username", "setup.invalid_url", "setup.invalid_email",
        "setup.invalid_password", "setup.username_taken", "setup.complete", "setup.warn_notty",
        "setup.env_path",
    ];
    for k in keys {
        assert!(en.get(k).is_some_and(|v| !v.is_empty()), "en missing {k}");
        assert!(zh.get(k).is_some_and(|v| !v.is_empty()), "zh missing {k}");
    }
}
```

(i18n.rs 测试模块内已 `use super::*`(既有模式);`parse_toml` 与 `EMBEDDED_EN`/`EMBEDDED_ZH` 均为模块顶层可见,直接引用即可。)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pichost-core i18n`
Expected: FAIL — setup.* 键缺失

- [ ] **Step 3: Write minimal implementation**

`en/messages.toml` 末尾追加:

```toml
"setup.welcome" = "Welcome to PicHost setup"
"setup.language" = "Interface language"
"setup.jwt_generated" = "A random JWT secret has been generated"
"setup.jwt_configured" = "JWT secret already configured, skipped"
"setup.public_url" = "Public URL"
"setup.public_url_skip" = "Public URL already configured, skipped"
"setup.admin_confirm" = "Create the first administrator account?"
"setup.admin_skip" = "Skipped admin creation. Register the first user via the web UI to become admin."
"setup.username" = "Admin username"
"setup.password" = "Admin password (min 8 characters)"
"setup.password_confirm" = "Confirm password"
"setup.email" = "Email (optional)"
"setup.invalid_username" = "Invalid username (1-64 characters)"
"setup.invalid_url" = "Invalid URL (must be http or https)"
"setup.invalid_email" = "Invalid email address"
"setup.invalid_password" = "Password must be at least 8 characters"
"setup.username_taken" = "Username or email already exists, please choose another"
"setup.complete" = "Admin account created"
"setup.warn_notty" = "First run detected but no interactive terminal; skipping setup wizard. Run `pichost-api --setup` or register the first user via the web UI."
"setup.env_path" = "No .env found; enter a path to create it"
```

`zh-CN/messages.toml` 末尾追加同 20 键(中文文案):

```toml
"setup.welcome" = "欢迎使用 PicHost 初始化向导"
"setup.language" = "界面语言"
"setup.jwt_generated" = "已自动生成随机 JWT secret"
"setup.jwt_configured" = "JWT secret 已配置,跳过"
"setup.public_url" = "公开访问地址"
"setup.public_url_skip" = "公开地址已配置,跳过"
"setup.admin_confirm" = "是否创建首个管理员账号?"
"setup.admin_skip" = "已跳过管理员创建。可稍后通过 Web 注册首个用户(将自动成为管理员)。"
"setup.username" = "管理员用户名"
"setup.password" = "管理员密码(至少 8 位)"
"setup.password_confirm" = "确认密码"
"setup.email" = "邮箱(可选)"
"setup.invalid_username" = "用户名无效(1-64 字符)"
"setup.invalid_url" = "地址无效(须为 http 或 https)"
"setup.invalid_email" = "邮箱格式无效"
"setup.invalid_password" = "密码至少需要 8 个字符"
"setup.username_taken" = "用户名或邮箱已存在,请换一个"
"setup.complete" = "管理员账号已创建"
"setup.warn_notty" = "检测到首次运行,但当前无交互终端,已跳过初始化向导。请运行 `pichost-api --setup` 或通过 Web 注册首个用户。"
"setup.env_path" = "未找到 .env,请输入要创建的路径"
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pichost-core i18n && cargo clippy --workspace -- -D warnings`
Expected: 全 PASS(含既有键集相等性),clippy 零警告

- [ ] **Step 5: Commit**

```bash
git add pichost-core/src/i18n/locales/en/messages.toml pichost-core/src/i18n/locales/zh-CN/messages.toml pichost-core/src/i18n.rs
git commit -m "feat: add setup.* i18n keys for first-run wizard"
```

**Verify:**
- `cargo test -p pichost-core i18n`
- `cargo clippy --workspace -- -D warnings`

---

### Task T6: Implement wizard orchestration and admin creation

**Breaking:** false(纯新增模块 + 新测试文件)

**Files:**
- Modify: `pichost-api/src/setup/mod.rs`(编排:maybe_run / run_wizard / phase1_config / resolve_env_path)
- Create: `pichost-api/src/setup/admin.rs`(管理员创建流程)
- Test: `pichost-api/tests/setup_wizard_test.rs`(SQLite 内存池,无需 Docker)

**Interfaces:**
- Consumes: T1、T2(`user_ops::{count_users, insert_user, hash_password}`)、T3(`Prompt`/`DialoguerPrompts`/`MockPrompts`/`MockReply`)、T4(`env_writer::{apply_env_file, generate_jwt_secret, probe_env_path, validate_jwt_secret, validate_public_url}`)、T5(`setup.*` 键)
- Produces(供 T7 消费,`pub`):
  - `pub enum TtyDecision { Run, SkipWarn, ForcedError }`
  - `pub fn decide_tty(forced: bool, is_tty: bool) -> Result<TtyDecision, &'static str>`
  - `pub fn should_run_wizard(user_count: i64, forced: bool) -> bool`
  - `pub fn choose_language(config: &AppConfig) -> Language`
  - `pub async fn maybe_run<DB: DbType>(pool: &Pool<DB>, config: &AppConfig, forced: bool) -> Result<Option<AppConfig>, Box<dyn Error + Send + Sync>>`(门控组合:首次判断 → TTY 判断 → run_wizard;where 子句同 T1)
  - `pub async fn run_wizard<DB: DbType>(pool, config, lang, prompts: &mut dyn Prompt) -> Result<Option<AppConfig>, Box<dyn Error + Send + Sync>>`(阶段1 配置 → 重载 config + I18n → 阶段2 管理员)
  - `pub async fn create_admin_flow<DB: DbType>(pool, config, lang, prompts) -> Result<bool, Box<dyn Error + Send + Sync>>`(admin.rs;返回是否创建)
  - 私有:`phase1_config(config, lang, prompts) -> Result<(), Box<dyn Error + Send + Sync>>`、`resolve_env_path(lang, prompts) -> Result<PathBuf, Box<dyn Error + Send + Sync>>`

**Acceptance Criteria:**
- given: `should_run_wizard(0, false)` / `should_run_wizard(3, false)` / `should_run_wizard(3, true)`
  when: 调用
  then: 返回 true / false / true
- given: `decide_tty(false, false)` / `decide_tty(true, false)` / `decide_tty(true, true)`
  when: 调用
  then: 返回 `Ok(SkipWarn)` / `Err("--setup requires an interactive terminal")` / `Ok(Run)`
- given: SQLite 内存池(0 用户)+ `MockPrompts`(URL + Confirm(true) + 用户名/密码/密码/邮箱)+ `PICHOST_ENV_FILE` 指向 tempdir
  when: `run_wizard(&pool, &cfg, Language::En, &mut prompts)` 并检查 .env 与 DB
  then: 返回 `Some(new_config)` 且 `new_config.server.public_url == "https://img.example.com"`;.env 含 `PICHOST_AUTH__JWT_SECRET=` 与 `PICHOST_SERVER__PUBLIC_URL=https://img.example.com`;`SELECT COUNT(*) FROM users WHERE is_admin = TRUE` == 1;`storage_prefix` 以 `users/` 开头
- given: 同一池已有 1 用户 + MockPrompts 不含管理员答案
  when: `run_wizard`
  then: 管理员步骤跳过,返回 `Some(config)`,用户数仍为 1
- given: 用户名冲突(先直插同名用户)
  when: `create_admin_flow` 用 MockPrompts 依次给冲突用户名 → 新用户名
  then: 冲突后重新提示,最终创建成功返回 `Ok(true)`,新用户行存在

**Regression:**
- `cargo test -p pichost-api --test auth_test`
- `cargo test -p pichost-api user_ops -- --exact`

- [ ] **Step 1: Write the failing test**

`pichost-api/tests/setup_wizard_test.rs`(SQLite 内存池模式参考 `tests/sqlite_smoke_test.rs`):

```rust
use pichost_api::setup::admin::create_admin_flow;
use pichost_api::setup::prompts::{MockPrompts, MockReply, Prompt};
use pichost_api::setup::{choose_language, decide_tty, run_wizard, should_run_wizard, TtyDecision};
use pichost_api::services::user_ops;
use pichost_core::config::AppConfig;
use pichost_core::db::{create_sqlite_pool, run_sqlite_migrations};
use pichost_core::i18n::Language;
use serial_test::serial;
use sqlx::SqlitePool;
use tempfile::TempDir;

async fn sqlite_pool() -> SqlitePool {
    let pool = create_sqlite_pool("sqlite::memory:", 5).await.unwrap();
    run_sqlite_migrations(&pool).await.unwrap();
    pool
}

fn base_config() -> AppConfig {
    let mut cfg = AppConfig::default();
    cfg.database.url = "sqlite::memory:".into();
    cfg.i18n.language = "en".into();
    cfg
}

/// 两个 run_wizard 测试均写/删全局 env(PICHOST_ENV_FILE 等),必须串行,
/// 避免并发互踩(serial_test 为既有 dev-dependency,AGENTS.md 约定)。
#[test]
fn gate_pure_functions() {
    assert!(should_run_wizard(0, false));
    assert!(!should_run_wizard(3, false));
    assert!(should_run_wizard(3, true));
    assert!(matches!(decide_tty(false, false).unwrap(), TtyDecision::SkipWarn));
    assert!(decide_tty(true, false).is_err());
    assert!(matches!(decide_tty(true, true).unwrap(), TtyDecision::Run));
    assert_eq!(choose_language(&base_config()), Language::En);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn run_wizard_writes_env_and_creates_admin() {
    let pool = sqlite_pool().await;
    let dir = TempDir::new().unwrap();
    let env_path = dir.path().join(".env");
    std::env::set_var("PICHOST_ENV_FILE", env_path.to_str().unwrap());
    std::env::remove_var("PICHOST_AUTH__JWT_SECRET");
    std::env::remove_var("PICHOST_SERVER__PUBLIC_URL");
    let cfg = base_config();
    let mut prompts = MockPrompts::new(vec![
        MockReply::Input("https://img.example.com".into()),
        MockReply::Confirm(true),
        MockReply::Input("admin".into()),
        MockReply::Password("admin12345".into()),
        MockReply::Password("admin12345".into()),
        MockReply::Input(String::new()),
    ]);
    let result = run_wizard(&pool, &cfg, Language::En, &mut prompts)
        .await
        .unwrap()
        .expect("wizard returns config");
    assert_eq!(result.server.public_url, "https://img.example.com");
    let content = std::fs::read_to_string(&env_path).unwrap();
    assert!(content.contains("PICHOST_SERVER__PUBLIC_URL=https://img.example.com"));
    assert!(content.contains("PICHOST_AUTH__JWT_SECRET="));
    let admins: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE is_admin = TRUE")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(admins, 1);
    let prefix: String = sqlx::query_scalar("SELECT storage_prefix FROM users LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(prefix.starts_with("users/"));
    std::env::remove_var("PICHOST_ENV_FILE");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[serial]
async fn run_wizard_skips_admin_when_users_exist() {
    let pool = sqlite_pool().await;
    let dir = TempDir::new().unwrap();
    let env_path = dir.path().join(".env");
    std::env::set_var("PICHOST_ENV_FILE", env_path.to_str().unwrap());
    std::env::remove_var("PICHOST_AUTH__JWT_SECRET");
    std::env::remove_var("PICHOST_SERVER__PUBLIC_URL");
    let cfg = base_config();
    let hash = user_ops::hash_password("password123").unwrap();
    user_ops::insert_user(&pool, "existing", &None, &hash, false, None).await.unwrap();
    let mut prompts = MockPrompts::new(vec![MockReply::Input("https://img.example.com".into())]);
    let result = run_wizard(&pool, &cfg, Language::En, &mut prompts)
        .await
        .unwrap()
        .expect("wizard returns config");
    assert_eq!(result.server.public_url, "https://img.example.com");
    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(total, 1);
    std::env::remove_var("PICHOST_ENV_FILE");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_admin_flow_reprompts_on_conflict() {
    let pool = sqlite_pool().await;
    let hash = user_ops::hash_password("password123").unwrap();
    user_ops::insert_user(&pool, "taken", &None, &hash, false, None).await.unwrap();
    let cfg = base_config();
    let mut prompts = MockPrompts::new(vec![
        MockReply::Confirm(true),
        MockReply::Input("taken".into()),
        MockReply::Password("admin12345".into()),
        MockReply::Password("admin12345".into()),
        MockReply::Input(String::new()),
        MockReply::Input("fresh".into()),
        MockReply::Password("admin12345".into()),
        MockReply::Password("admin12345".into()),
        MockReply::Input(String::new()),
    ]);
    let created = create_admin_flow(&pool, &cfg, Language::En, &mut prompts).await.unwrap();
    assert!(created);
    let fresh: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE username = 'fresh'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(fresh, 1);
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pichost-api --test setup_wizard_test`
Expected: FAIL — setup 模块函数不存在(编译错误)

- [ ] **Step 3: Write minimal implementation**

`pichost-api/src/setup/admin.rs`(create_admin_flow ≤50 行):

```rust
use pichost_core::config::AppConfig;
use pichost_core::db::DbErrorKind;
use pichost_core::i18n::{I18n, Language};
use pichost_core::DbType;
use sqlx::Pool;
use std::error::Error;

use crate::services::user_ops;
use super::prompts::Prompt;

pub async fn create_admin_flow<DB: DbType>(
    pool: &Pool<DB>,
    config: &AppConfig,
    lang: Language,
    prompts: &mut dyn Prompt,
) -> Result<bool, Box<dyn Error + Send + Sync>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    let i18n = I18n::global();
    if !prompts.confirm(&i18n.t(lang, "setup.admin_confirm"), true)? {
        println!("{}", i18n.t(lang, "setup.admin_skip"));
        return Ok(false);
    }
    loop {
        let username = prompts.input(&i18n.t(lang, "setup.username"), None)?;
        if username.is_empty() || username.len() > 64 {
            println!("{}", i18n.t(lang, "setup.invalid_username"));
            continue;
        }
        let password = prompts.password(
            &i18n.t(lang, "setup.password"),
            Some(&i18n.t(lang, "setup.password_confirm")),
        )?;
        if password.len() < 8 {
            println!("{}", i18n.t(lang, "setup.invalid_password"));
            continue;
        }
        let email_raw = prompts.input(&i18n.t(lang, "setup.email"), None)?;
        let email = if email_raw.is_empty() { None } else { Some(email_raw) };
        if let Some(e) = &email {
            if !e.contains('@') {
                println!("{}", i18n.t(lang, "setup.invalid_email"));
                continue;
            }
        }
        let hash = user_ops::hash_password(&password)?;
        let quota = if config.upload.storage_quota_default > 0 {
            Some(config.upload.storage_quota_default as i64)
        } else {
            None
        };
        match user_ops::insert_user(pool, &username, &email, &hash, true, quota).await {
            Ok(user_id) => {
                let prefix = format!("users/{user_id}");
                let _ = sqlx::query("UPDATE users SET storage_prefix = $1 WHERE id = $2")
                    .bind(&prefix)
                    .bind(user_id)
                    .execute(pool)
                    .await;
                println!("{}", i18n.t(lang, "setup.complete"));
                return Ok(true);
            }
            Err(e) if pichost_core::db::db_error_kind(&e) == DbErrorKind::UniqueViolation => {
                println!("{}", i18n.t(lang, "setup.username_taken"));
            }
            Err(e) => return Err(e.into()),
        }
    }
}
```

`pichost-api/src/setup/mod.rs` 替换为完整编排(每个函数 ≤50 行):

```rust
pub mod admin;
pub mod env_writer;
pub mod prompts;

use std::error::Error;
use std::io::IsTerminal;
use std::path::PathBuf;

use pichost_core::config::{load_config, AppConfig};
use pichost_core::i18n::{I18n, Language};
use pichost_core::DbType;
use sqlx::Pool;

use crate::services::user_ops;
use prompts::Prompt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TtyDecision {
    Run,
    SkipWarn,
    ForcedError,
}

pub fn decide_tty(forced: bool, is_tty: bool) -> Result<TtyDecision, &'static str> {
    if is_tty {
        return Ok(TtyDecision::Run);
    }
    if forced {
        return Err("--setup requires an interactive terminal");
    }
    Ok(TtyDecision::SkipWarn)
}

pub fn should_run_wizard(user_count: i64, forced: bool) -> bool {
    forced || user_count == 0
}

pub fn choose_language(config: &AppConfig) -> Language {
    Language::from_str_opt(&config.i18n.language)
}

pub async fn maybe_run<DB: DbType>(
    pool: &Pool<DB>,
    config: &AppConfig,
    forced: bool,
) -> Result<Option<AppConfig>, Box<dyn Error + Send + Sync>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    if !should_run_wizard(user_ops::count_users(pool).await?, forced) {
        return Ok(None);
    }
    let lang = choose_language(config);
    match decide_tty(forced, std::io::stdin().is_terminal())? {
        TtyDecision::SkipWarn => {
            let msg = I18n::global().t(lang, "setup.warn_notty");
            tracing::warn!("{msg}");
            return Ok(None);
        }
        TtyDecision::ForcedError => return Err("--setup requires an interactive terminal".into()),
        TtyDecision::Run => {}
    }
    let mut prompts = prompts::DialoguerPrompts;
    run_wizard(pool, config, lang, &mut prompts).await
}

pub async fn run_wizard<DB: DbType>(
    pool: &Pool<DB>,
    config: &AppConfig,
    lang: Language,
    prompts: &mut dyn Prompt,
) -> Result<Option<AppConfig>, Box<dyn Error + Send + Sync>>
where
    for<'c> &'c mut <DB as sqlx::Database>::Connection: sqlx::Executor<'c, Database = DB>,
    for<'q> <DB as sqlx::Database>::Arguments<'q>: sqlx::IntoArguments<'q, DB>,
    (uuid::Uuid,): crate::db::DbRow<DB>,
    (i64,): crate::db::DbRow<DB>,
    usize: sqlx::ColumnIndex<DB::Row>,
    str: sqlx::Type<DB>,
    for<'q> &'q str: sqlx::Encode<'q, DB>,
    uuid::Uuid: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<String>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    Option<i64>: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    String: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    bool: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
    i64: for<'q> sqlx::Encode<'q, DB> + for<'r> sqlx::Decode<'r, DB> + sqlx::Type<DB>,
{
    phase1_config(config, lang, prompts)?;
    let new_config = load_config()?;
    I18n::reload_global(
        Language::from_str_opt(&new_config.i18n.language),
        new_config.i18n.locales_dir.clone(),
    );
    if user_ops::count_users(pool).await? == 0 {
        admin::create_admin_flow(pool, &new_config, lang, prompts).await?;
    }
    Ok(Some(new_config))
}

fn phase1_config(
    config: &AppConfig,
    lang: Language,
    prompts: &mut dyn Prompt,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    let i18n = I18n::global();
    let mut updates: Vec<(String, String)> =
        vec![("PICHOST_I18N_LANGUAGE".into(), lang.as_str().into())];
    let jwt = std::env::var("PICHOST_AUTH__JWT_SECRET").ok();
    if jwt.is_none_or(|s| !env_writer::validate_jwt_secret(&s)) {
        updates.push(("PICHOST_AUTH__JWT_SECRET".into(), env_writer::generate_jwt_secret()));
        println!("{}", i18n.t(lang, "setup.jwt_generated"));
    } else {
        println!("{}", i18n.t(lang, "setup.jwt_configured"));
    }
    if std::env::var("PICHOST_SERVER__PUBLIC_URL").is_err() {
        loop {
            let url = prompts.input(&i18n.t(lang, "setup.public_url"), Some(&config.server.public_url))?;
            if env_writer::validate_public_url(&url) {
                updates.push(("PICHOST_SERVER__PUBLIC_URL".into(), url));
                break;
            }
            println!("{}", i18n.t(lang, "setup.invalid_url"));
        }
    }
    let path = resolve_env_path(lang, prompts)?;
    let refs: Vec<(&str, &str)> = updates.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    env_writer::apply_env_file(&path, &refs)?;
    for (k, v) in &updates {
        std::env::set_var(k, v);
    }
    Ok(())
}

fn resolve_env_path(
    lang: Language,
    prompts: &mut dyn Prompt,
) -> Result<PathBuf, Box<dyn Error + Send + Sync>> {
    let explicit = std::env::var("PICHOST_ENV_FILE").ok();
    let cwd = std::env::current_dir()?;
    if let Some(p) = env_writer::probe_env_path(
        explicit.as_deref().map(std::path::Path::new),
        std::path::Path::new("/etc/pichost"),
        &cwd,
    ) {
        return Ok(p);
    }
    let prompt = I18n::global().t(lang, "setup.env_path");
    let answer = prompts.input(&prompt, Some("/etc/pichost/.env"))?;
    Ok(PathBuf::from(answer))
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pichost-api --test setup_wizard_test && cargo test -p pichost-api --test auth_test && cargo clippy --workspace -- -D warnings`
Expected: 4 PASS(3 集成 + 1 纯函数),auth 回归 PASS,clippy 零警告

- [ ] **Step 5: Commit**

```bash
git add pichost-api/src/setup/mod.rs pichost-api/src/setup/admin.rs pichost-api/tests/setup_wizard_test.rs
git commit -m "feat: first-run wizard orchestration and admin creation"
```

**Verify:**
- `cargo test -p pichost-api --test setup_wizard_test`
- `cargo test -p pichost-api --test auth_test`
- `cargo clippy --workspace -- -D warnings`

---

### Task T7: Wire wizard into PG and sqlite startup paths

**Breaking:** false(`run_lite_from_env()` 无参包装保留,Windows service.rs 调用点零改动;新增 `run_lite_from_env_forced`)

**Files:**
- Modify: `pichost-api/src/main.rs`(CLI 分发 + run_app 双分支接线)
- Modify: `pichost-api/src/lib.rs`(`run_lite_from_env_forced(forced)` 新函数;原 `run_lite_from_env()` 保留为 `false` 薄包装)
- Test: `pichost-api/tests/setup_wizard_test.rs`(追加 maybe_run 入口 3 测)

**Interfaces:**
- Consumes: T6(`setup::maybe_run`)
- Produces:
  - `pub async fn run_lite_from_env_forced(forced: bool) -> Result<(), Box<dyn Error>>`(lib.rs;原 `run_lite_from_env()` 改为 `run_lite_from_env_forced(false).await` 一行包装,Windows service.rs 调用点零改动)
  - `main.rs`:`Run → run_app(false)`、`Setup → run_app(true)`;PG 分支 `maybe_run` 后 `cfg.unwrap_or(config)` 传入 `run_with`;sqlite 分支 `run_lite_from_env_forced(forced)`

**Acceptance Criteria:**
- given: `maybe_run` 在用户数 == 0 且 stdin 非 TTY(cargo test 管道)
  when: 自动触发(forced=false)
  then: 返回 `Ok(None)`(跳过 + WARN,不阻塞启动)
- given: `maybe_run` 在用户数 == 0 且 forced=true 且 stdin 非 TTY
  when: 调用
  then: 返回 Err("--setup requires an interactive terminal")
- given: `maybe_run` 在用户数 > 0 且 forced=false
  when: 调用
  then: 返回 `Ok(None)`(首次判断短路)
- given: `pichost-api --setup` 在非 TTY 环境运行(编译后二进制)
  when: 手动验证
  then: 进程非 0 退出且 stderr 含 "interactive terminal"(maybe_run Err 冒泡;CI 无 TTY 无法自动化,以编译 + maybe_run 测试覆盖)
- given: Windows service 编译(cfg(windows))
  when: 检查 `service.rs` 调用点
  then: 仍调用无参 `run_lite_from_env()`,零改动

**Regression:**
- `cargo test -p pichost-api --test setup_wizard_test`
- `cargo test --workspace`(全量无 infra 套件 416 pass)

- [ ] **Step 1: Write the failing test**

`pichost-api/tests/setup_wizard_test.rs` 追加:

```rust
use pichost_api::setup::maybe_run;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn maybe_run_non_tty_first_run_skips() {
    let pool = sqlite_pool().await;
    let result = maybe_run(&pool, &base_config(), false).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn maybe_run_forced_non_tty_errors() {
    let pool = sqlite_pool().await;
    let err = maybe_run(&pool, &base_config(), true).await.unwrap_err();
    assert!(err.to_string().contains("interactive terminal"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn maybe_run_existing_users_short_circuits() {
    let pool = sqlite_pool().await;
    let hash = user_ops::hash_password("password123").unwrap();
    user_ops::insert_user(&pool, "someone", &None, &hash, false, None).await.unwrap();
    let result = maybe_run(&pool, &base_config(), false).await.unwrap();
    assert!(result.is_none());
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p pichost-api --test setup_wizard_test maybe_run`
Expected: FAIL — `maybe_run` 尚不存在(编译错误)

- [ ] **Step 3: Write minimal implementation**

`pichost-api/src/lib.rs` 拆分(原 `run_lite_from_env` 保留无参包装,新增带参实现):

```rust
/// SQLite lite 模式启动链路(前台 run_app 与 Windows 服务共用;无强制向导)
pub async fn run_lite_from_env() -> Result<(), Box<dyn std::error::Error>> {
    run_lite_from_env_forced(false).await
}

/// 同 `run_lite_from_env`,但可强制运行初始化向导(--setup)
pub async fn run_lite_from_env_forced(forced: bool) -> Result<(), Box<dyn std::error::Error>> {
    let config = load_config()?;
    I18n::init_global(
        Language::from_str_opt(&config.i18n.language),
        config.i18n.locales_dir.clone(),
    );
    let pool =
        db::create_sqlite_pool(&config.database.url, config.database.max_connections).await?;
    db::run_sqlite_migrations(&pool).await?;
    let config = crate::setup::maybe_run(&pool, &config, forced)
        .await?
        .unwrap_or(config);
    app::run_with_sqlite(config, pool).await
}
```

`pichost-api/src/main.rs`:

```rust
use pichost_api::{app, cache, db, run_lite_from_env_forced};
use pichost_core::config::{load_config, DatabaseMode};
use pichost_core::i18n::{I18n, Language};

mod cli;
#[cfg(windows)]
mod service;

match cmd {
    cli::CliCommand::Run => return run_app(false).await,
    cli::CliCommand::Setup => return run_app(true).await,
    cli::CliCommand::Help => { /* 不变 */ }
    other => { /* 不变 */ }
}
```

```rust
async fn run_app(forced: bool) -> Result<(), Box<dyn std::error::Error>> {
    // dotenv + tracing 初始化不变
    let config = load_config()?;
    I18n::init_global(
        Language::from_str_opt(&config.i18n.language),
        config.i18n.locales_dir.clone(),
    );

    match config.database.mode {
        DatabaseMode::Postgres => {
            let pool =
                db::create_pg_pool(&config.database.url, config.database.max_connections).await?;
            db::run_pg_migrations(&pool).await?;
            let cache_pool = cache::create_pool(&config.redis.url, config.redis.pool_size as usize);
            let queue_pool = cache::create_pool(&config.redis.url, config.redis.pool_size as usize);
            let config = pichost_api::setup::maybe_run(&pool, &config, forced)
                .await?
                .unwrap_or(config);
            app::run_with::<sqlx::Postgres>(config, pool, cache_pool, queue_pool).await
        }
        DatabaseMode::Sqlite => run_lite_from_env_forced(forced).await,
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p pichost-api --test setup_wizard_test && cargo test --workspace && cargo clippy --workspace -- -D warnings`
Expected: 新增 3 PASS,全 workspace 无 infra 套件 PASS,clippy 零警告

- [ ] **Step 5: Commit**

```bash
git add pichost-api/src/main.rs pichost-api/src/lib.rs pichost-api/tests/setup_wizard_test.rs
git commit -m "feat: wire first-run wizard into startup paths"
```

**Verify:**
- `cargo test -p pichost-api --test setup_wizard_test`
- `cargo test --workspace`
- `cargo clippy --workspace -- -D warnings`

---

### Task T8: Bump version to 0.24.0

**Breaking:** false(版本元数据变更)

> TDD 豁免说明:版本变更无红-绿测试周期,以 `scripts/tests/version_check_test.sh` 为绿门(既有脚本)。

**Files:**
- Modify: `Cargo.toml`(workspace `version = "0.23.0"` → `"0.24.0"`)
- Modify: `web-ui/package.json`(version 对齐 0.24.0)
- Modify: `CHANGELOG.md`(0.24.0 条目,Keep a Changelog)

**Interfaces:**
- Produces: 版本 0.24.0(workspace + 前端 + CHANGELOG 对齐)

**Acceptance Criteria:**
- given: workspace Cargo.toml
  when: 检查 version 字段
  then: 等于 "0.24.0"
- given: web-ui/package.json
  when: 检查 version 字段
  then: 等于 "0.24.0"
- given: CHANGELOG.md
  when: 检查最新条目
  then: 含 0.24.0 且描述向导特性
- given: 版本一致性脚本
  when: 运行
  then: 通过

**Regression:**
- `bash scripts/tests/version_check_test.sh`
- `cargo test --workspace`(Cargo.lock 随 build 自动更新)

- [ ] **Step 1: 确认基线**

Run: `cargo tree -p pichost-api --depth 0`(确认当前 0.23.0;版本变更无独立红测试,以版本检查脚本为绿门)

- [ ] **Step 2: 修改版本**

`Cargo.toml`(workspace 根):

```toml
version = "0.24.0"
```

`web-ui/package.json`:

```json
"version": "0.24.0"
```

`CHANGELOG.md` 顶部追加:

```markdown
## [0.24.0] - 2026-08-15

### Added
- First-run terminal setup wizard: on initial startup (no users) with an
  interactive terminal, guide configuration of JWT secret / public URL /
  UI language and create the first administrator account
- `pichost-api --setup` flag to force-run the wizard; non-TTY environments
  skip with a warning (forced mode errors out)
- `.env` writes are idempotent (single/double underscore variant dedup)
  and atomic (temp file + rename); `PICHOST_ENV_FILE` env var overrides
  the probe order (`PICHOST_ENV_FILE` → `/etc/pichost/.env` → CWD `.env`)
```

- [ ] **Step 3: 验证**

Run: `cargo build --workspace && bash scripts/tests/version_check_test.sh && cargo test --workspace && cargo clippy --workspace -- -D warnings`
Expected: 全 PASS(Cargo.lock 同步更新)

- [ ] **Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock web-ui/package.json CHANGELOG.md
git commit -m "chore: bump version to 0.24.0"
```

**Verify:**
- `bash scripts/tests/version_check_test.sh`
- `cargo test --workspace`
- `cargo clippy --workspace -- -D warnings`

---

### Task T9: Sync docs and run full verification

**Breaking:** false(纯文档)

> TDD 豁免说明:文档同步无红-绿测试周期,以 `scripts/tests/docs_check_test.sh` + 全量门禁为绿门(既有脚本)。

**Files:**
- Modify: `README.md`(Quick Start 首次启动向导说明;配置表新增 `PICHOST_ENV_FILE`;Features 勾选)
- Modify: `AGENTS.md`(版本 0.24.0、setup 模块、`--setup` 标志、`PICHOST_ENV_FILE`、crate 边界)
- Modify: `.omo/summary/summary_and_next.md`(新阶段小节 + 待实施表)

**Interfaces:**
- Consumes: T8(版本 0.24.0)
- Produces: 文档同步提交(`docs: auto-sync ...` 单提交,AGENTS.md 强制)

**Acceptance Criteria:**
- given: README.md
  when: 检查
  then: 含 "first-run setup wizard" 说明、`PICHOST_ENV_FILE` 配置行、Features 新勾选项
- given: AGENTS.md
  when: 检查
  then: 版本 0.24.0、setup 模块、`--setup`、`PICHOST_ENV_FILE` 已记录
- given: docs 检查脚本
  when: 运行
  then: 通过
- given: 全量门禁
  when: 运行 `cargo test --workspace` + clippy + `npm run build`
  then: 全部通过

**Regression:**
- `bash scripts/tests/docs_check_test.sh`
- `cargo test --workspace`

- [ ] **Step 1: README.md 更新**

- Quick Start(裸机/首次启动段)追加:

```markdown
On first start (no users yet) with an interactive terminal, `pichost-api`
runs a setup wizard: it configures the JWT secret, public URL and UI
language (written to `.env`), then offers to create the first admin
account. Non-TTY environments (systemd/Docker) skip the wizard with a
warning — register the first user via the web UI instead. Re-run anytime
with `pichost-api --setup`.
```

- 配置表追加行:

```markdown
| `PICHOST_ENV_FILE` | — | — | Wizard `.env` write target override (probe order: `PICHOST_ENV_FILE` → `/etc/pichost/.env` → CWD `.env`) |
```

- Features 列表勾选:

```markdown
- [x] **First-run setup wizard** — terminal wizard on initial startup (JWT/public URL/language + first admin), `--setup` force flag, non-TTY skip
```

- [ ] **Step 2: AGENTS.md 更新**

- 版本:0.23.0 → 0.24.0(顶部 Version 行 + 相关小节)
- 新配置表行:`PICHOST_ENV_FILE` — 向导 .env 写入目标覆盖
- 架构说明新增:`setup` 模块(prompts/env_writer/admin + user_ops 提取)、`pichost-api --setup` 标志、首次启动向导行为(用户数==0 + TTY → 向导;非 TTY → WARN 跳过)
- API 端点表无变化(纯启动流程特性)

- [ ] **Step 3: `.omo/summary/summary_and_next.md` 更新**

- 新增 "## 首次安装终端初始化向导 ✅ (本次完成)" 小节:特性、验证结果(cargo test / clippy / npm build)、版本 0.24.0
- 待实施表清理(如有相关行)

- [ ] **Step 4: 全量验证**

Run: `cargo test --workspace && cargo clippy --workspace -- -D warnings && cd web-ui && npm run build`
Expected: 全 PASS(416+ 无 infra 测试;前端构建通过)

- [ ] **Step 5: Commit(单提交)**

```bash
git add README.md AGENTS.md .omo/summary/summary_and_next.md
git commit -m "docs: auto-sync README, AGENTS, summary after first-run wizard completion"
```

**Verify:**
- `bash scripts/tests/docs_check_test.sh`
- `cargo test --workspace`
- `cargo clippy --workspace -- -D warnings`
- `cd web-ui && npm run build`

