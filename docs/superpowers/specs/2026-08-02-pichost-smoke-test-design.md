# PicHost API Smoke Test Design Guide

> **状态**: 已实施
> **日期**: 2026-08-02
> **目标**: 为 PicHost API 建立冒烟测试设计模式，确保每次新特性开发包含 API 级端到端测试。

## 1. 概述

### 1.1 什么是冒烟测试（Smoke Test）

在 PicHost 项目中，**冒烟测试**指针对每个 API 端点编写的一个或多个端到端集成测试，通过生产路由 (`configure_app`) 驱动真实的 PostgreSQL + Redis + MinIO 服务栈，验证请求-响应全链路行为。

与单元测试的区别：
- **单元测试**（`src/` 中的 `#[cfg(test)]`）：测试纯逻辑函数，不需要外部服务
- **冒烟测试**（`tests/` 中的集成测试）：测试完整的 HTTP 请求-响应管道，需要 Docker 基础设施

### 1.2 当前覆盖

- **50 个 API 端点**全部有集成测试覆盖（`pichost-api/tests/`）
- **555 个测试**通过 Docker PG+Redis+MinIO 运行（`cargo test --workspace -- --include-ignored`）
- **313 个测试**不需要基础设施（仅单元测试）
- **CI 工作流**：`.github/workflows/smoke-test.yml` 在 PR 合入 `main` 时自动运行全部 555 个测试

## 2. 测试架构

### 2.1 测试 Harness

`pichost-api/tests/common/mod.rs` 提供共享测试工具：

```
test_app()           → 构建完整的 TestApp（生产路由 + 真实 PG/Redis + 临时存储目录）
create_user()        → 创建普通用户并返回 (username, token, user_id)
create_admin()       → 创建管理员用户并返回 (token, user_id)
send_json()          → 发送 JSON 请求并返回 (StatusCode, Value)
send_raw()           → 发送原始请求（如 multipart 上传）并返回响应
tiny_png()           → 生成 1x1 PNG 字节（用于上传测试）
multipart_image()    → 构建 multipart/form-data 上传体
```

### 2.2 测试结构

```rust
// pichost-api/tests/example_test.rs
mod common;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires running PostgreSQL and Redis"]
async fn test_new_endpoint_success_path() {
    let app = common::test_app().await;
    let (_username, token, _user_id) = common::create_user(&app, "test").await;

    let body = serde_json::json!({ "field": "value" });
    let (status, resp) = common::send_json(
        &app, Method::POST, "/api/v1/new-endpoint",
        Some(&token), &body,
    ).await;

    assert_eq!(status, 200);
    assert_eq!(resp["result"], "expected_value");
}
```

## 3. 新特性开发流程

### 3.1 开发之前：设计冒烟测试

在新特性开始编码之前，先完成以下检查清单：

- [ ] **列出涉及的新 API 端点**（路径、方法、请求体、响应体）
- [ ] **列出涉及修改的现有端点**（行为变更需要更新测试）
- [ ] **为每个端点设计至少一个成功路径测试**（happy path）
- [ ] **为每个端点设计关键错误路径测试**（认证失败、参数无效、资源不存在、权限不足）
- [ ] **确认测试文件归属**（新文件 `tests/new_feature_test.rs` 或扩展现有文件）
- [ ] **确认需要哪些辅助函数**（是否需要在 `common/mod.rs` 中添加新的 helper）

### 3.2 开发中：编写测试

遵循 TDD 原则（红-绿-重构）：

1. **RED**: 先编写冒烟测试，确认测试因为功能缺失而失败
2. **GREEN**: 实现最小代码使测试通过
3. **REFACTOR**: 重构代码，保持测试通过

### 3.3 开发后：验证

```bash
# 1. 运行全部测试确认无回归
cargo test --workspace -- --include-ignored

# 2. Clippy 零警告
cargo clippy --workspace -- -D warnings

# 3. 确认新测试出现在 CI 中
# smoke-test.yml 会自动运行 --include-ignored
```

## 4. 测试分类与命名规范

### 4.1 文件命名

| 模式 | 用途 | 示例 |
|------|------|------|
| `{domain}_test.rs` | 核心功能测试 | `auth_test.rs`, `images_test.rs` |
| `gaps_{domain}_test.rs` | 边界情况和补充覆盖 | `gaps_auth_test.rs`, `gaps_images_test.rs` |
| `gaps2_{domain}_test.rs` | 第二轮补充覆盖 | `gaps2_admin_test.rs` |

### 4.2 测试函数命名

采用 `{domain}_{action}_{condition}` 模式：

```rust
async fn test_upload_rejects_non_image()      // 动作 + 预期结果
async fn test_login_with_wrong_password_fails() // 动作 + 条件 + 预期
async fn test_admin_cannot_delete_self()       // 角色 + 动作 + 约束
```

### 4.3 `#[ignore]` 属性

所有需要外部服务（PG/Redis/MinIO）的测试必须标记：

```rust
#[ignore = "requires running PostgreSQL and Redis"]
```

需要 S3/MinIO 的测试额外标记：

```rust
#[ignore = "requires running PostgreSQL, Redis, and MinIO"]
```

**注意**：如果测试同时依赖 MinIO，测试函数应在环境变量缺失时优雅跳过而非 panic。参考 `rustfs_test.rs` 中的 `get_config()` 模式。

## 5. 现有覆盖状态

### 5.1 完全覆盖

所有 50 个 API 端点均有集成测试。

### 5.2 已知缺口

| 缺口 | 优先级 | 说明 |
|------|--------|------|
| OAuth callback 成功路径 | 低 | 需要 mock OAuth provider HTTP 交换，当前仅测试错误路径 |
| OAuth link 成功路径 | 低 | 同上 |
| 第三方 OAuth provider 集成测试 | 低 | 需要真实 OAuth App 凭证，不适合 CI 自动化 |

### 5.3 新特性开发时的覆盖要求

- **CRUD 端点**: 至少覆盖 create success + create validation error + read success + read not_found + update success + update not_found + delete success + delete not_found
- **认证端点**: 至少覆盖 success path + wrong credentials + rate limiting behavior
- **管理端点**: 至少覆盖 admin success + non-admin 403 + not_found
- **公共端点**: 至少覆盖 success + not_found + cache headers

## 6. CI 工作流

### 6.1 smoke-test.yml

- **触发**: PR 合入 `main` 分支（open, sync, reopen）+ push 到 `main`
- **基础设施**: PostgreSQL 18 + Redis 8 + MinIO（bucket `pichost` 自动创建）
- **命令**: `cargo test --workspace -- --include-ignored`
- **质量门**: `cargo clippy --workspace -- -D warnings`
- **超时**: 25 分钟

### 6.2 PR 合入流程

```
开发新特性
    ↓
编写冒烟测试（TDD 红-绿-重构）
    ↓
本地验证: cargo test --workspace -- --include-ignored
    ↓
创建 PR → 自动触发 smoke-test.yml
    ↓
CI 全部通过 → 合入 main
```

## 7. 常见问题

### Q: 我的测试需要新的数据库迁移怎么办？
A: 测试 harness 在每次 `test_app()` 调用时自动运行 `sqlx::migrate!()`，迁移会自动应用。确保迁移 ID 唯一且可重复执行。

### Q: 测试之间如何隔离？
A: 每个 `test_app()` 创建独立的临时存储目录和独立的 PG 连接池（max 5 连接）。用户数据通过唯一用户名隔离（`short_username()` 函数生成 UUID 后缀）。测试结束后临时目录自动清理。

### Q: 如何在测试中模拟文件上传？
A: 使用 `common::tiny_png()` 生成测试图片，`common::multipart_image()` 构建 multipart body，通过 `common::send_raw()` 发送。

### Q: CI 中 MinIO bucket 如何创建？
A: `smoke-test.yml` 在测试步骤前通过 `minio/mc` 容器运行 `mc mb --ignore-existing local/pichost` 自动创建。

## 8. 参考

- 测试 harness: `pichost-api/tests/common/mod.rs`
- CI 工作流: `.github/workflows/smoke-test.yml`
- 项目设计文档: `docs/superpowers/specs/2026-07-11-pichost-design.md`
- P4 设计文档: `docs/superpowers/specs/2026-07-19-pichost-p4-design.md`
- AGENTS.md: 项目开发指南和 API 端点列表

---

> **维护规则**: 每次新特性开发完成后，更新本文档 §5 的覆盖状态表格。
> **版本历史**: v0.1 — 2026-08-02 初始版本（覆盖 P0–P4-I 全部端点）。
