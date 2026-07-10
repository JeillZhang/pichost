# PicHost 图床系统设计文档

> **日期**: 2026-07-11
> **项目**: `pichost-rust`
> **技术栈**: React 19 + Rust 1.96 (Axum 0.8, Tokio 1.52) + PostgreSQL 18 + Redis 8.0
> **部署**: Docker Compose
> **规模目标**: 500-2000 并发, 中型图床服务
> **使用场景**: 个人/团队自用, 多用户独立空间

---

## 1. 目标与范围

### 1.1 概述

PicHost 是一个面向**个人/团队自用**的图床系统，支持**多用户独立空间**，提供图片上传、外链分享、Markdown 引用等功能。每个用户拥有独立的存储空间和上传管理。

### 1.2 核心能力

- **多用户独立空间**: 每个用户拥有独立的图片库和存储路径
- **图片上传**: 文件拖拽上传，支持全格式图片
- **外链分享**: 公开固定 URL，无需认证即可访问
- **全格式链接**: URL / Markdown / HTML / BBCode 一键复制
- **图片处理**: 异步缩略图生成 + WebP 转换
- **存储可配置**: LocalFS 和 RustFS (S3 兼容) 可选/并行
- **高可用**: 多实例水平扩展 + Nginx 负载均衡
- **主题切换**: 亮色/暗色/跟随系统，毛玻璃质感

### 1.3 非目标

- 不提供公开注册（仅邀请制）
- 不做图片社交功能（点赞、评论）
- 不提供 CDN 集成（可后期通过 Nginx + 外部 CDN 配合）
- 不提供视频/文档托管

---

## 2. 架构概览

### 2.1 系统架构图

```
┌─────────────────────────────────────────────────┐
│             React Frontend (Vite)                │
│    React 19 + shadcn/ui + TanStack Query         │
└──────────────────┬──────────────────────────────┘
                   │ REST API (JSON)
                   │  ┌────────────────────────────┐
                   │  │   Nginx (反向代理 + 负载均衡) │
                   │  │   静态资源 serve + 图片缓存    │
                   │  └────┬───────┬────────────────┘
                   │       │       │
        ┌──────────▼───────▼───────┐   (水平扩展)
        │  Rust Backend (Axum 0.8) │
        │  Tokio 1.52              │
        │  ┌──────┐ ┌───────┐      │  ┌──────────────────┐
        │  │ Auth │ │Upload │      │  │  PostgreSQL 18    │
        │  │JWT   │ │ 管线   │      │──┤  用户 + 图片元数据  │
        │  └──────┘ └───┬───┘      │  └──────────────────┘
        │  ┌──────┐ ┌───┴───┐      │
        │  │Users │ │Images │      │  ┌──────────────────┐
        │  │ CRUD │ │ CRUD  │      │  │  Redis 8.0        │
        │  └──────┘ └───────┘      │──┤  缓存 + 队列 + 限流 │
        │  ┌────────────────┐      │  └──────────────────┘
        │  │ StorageRouter  │      │
        │  │ (Trait 抽象)    │──────┼──► LocalFS / RustFS
        │  └────────────────┘      │
        └──────┬───────────────────┘
               │  LPUSH 任务
               ▼
        ┌──────────────────────┐
        │  pichost-worker      │
        │  独立二进制进程          │
        │  缩略图 + WebP + 压缩  │
        │  从 Redis BRPOP 消费    │
        └──────────────────────┘
```

**关键设计决策**:
- Api 与 Worker 是**同一 workspace 下两个独立 bin**，共享 `pichost-core` lib
- Api 是无状态的（状态在 PostgreSQL + Redis 中），可以水平扩展
- Worker 通过 Redis List 消费任务, 也可水平扩展
- Api 上传管线: 接收→校验→写存储→写 DB→入队列→立即返回

### 2.2 项目结构

```
pichost-rust/
├── Cargo.toml                        # workspace 根
├── Cargo.lock
├── crates/
│   ├── pichost-core/                 # 共享核心 (lib, 所有 crate 共用)
│   │   ├── src/
│   │   │   ├── models/               # User, Image, UploadTask 领域模型
│   │   │   ├── storage/
│   │   │   │   ├── mod.rs            # StorageBackend trait 定义
│   │   │   │   ├── local.rs          # LocalStorage 实现 (tokio::fs)
│   │   │   │   ├── rustfs.rs         # RustfsStorage 实现 (aws-sdk-s3)
│   │   │   │   └── router.rs         # StorageRouter (多后端路由)
│   │   │   ├── config.rs             # 配置结构 (figment 多层配置)
│   │   │   └── error.rs              # 统一错误类型 (thiserror + AppError)
│   │   └── Cargo.toml
│   ├── pichost-api/                  # Axum 后端主进程 (bin)
│   │   ├── src/
│   │   │   ├── main.rs               # 入口: 绑定 0.0.0.0:3000, 挂载路由
│   │   │   ├── routes/
│   │   │   │   ├── mod.rs            # Router 组装和 /api/v1 前缀
│   │   │   │   ├── auth.rs           # /auth/* 注册、登录、刷新、登出
│   │   │   │   ├── images.rs         # /images/* 上传、列表、详情、删除
│   │   │   │   └── users.rs          # /users/* 设置、管理
│   │   │   ├── middleware/
│   │   │   │   ├── auth.rs           # JWT 验证中间件 (从 header 提取 + 黑名单检查)
│   │   │   │   ├── rate_limit.rs     # Redis 计数器限流
│   │   │   │   └── security.rs       # 安全头 (CSP, HSTS, X-Frame-Options 等)
│   │   │   ├── services/
│   │   │   │   ├── upload_service.rs # 上传管线: 校验→SHA256→去重→存储→入库→入队
│   │   │   │   └── image_service.rs  # 图片 CRUD + 链接生成
│   │   │   ├── db/
│   │   │   │   ├── mod.rs            # sqlx 连接池管理
│   │   │   │   └── queries/          # 编译期校验的 SQL 查询
│   │   │   └── cache/
│   │   │       └── mod.rs            # Redis 缓存封装 (deadpool-redis)
│   │   └── Cargo.toml
│   └── pichost-worker/               # 图片处理 Worker (bin)
│       ├── src/
│       │   ├── main.rs               # 入口: 启动 BRPOP 循环 + 并发池
│       │   ├── queue/
│       │   │   └── mod.rs            # Redis 队列消费 (BRPOPLPUSH 原子移动)
│       │   ├── processor/
│       │   │   ├── mod.rs            # 处理器调度
│       │   │   ├── thumbnail.rs      # 缩略图生成 (image crate)
│       │   │   ├── webp.rs           # WebP 转换 (image-webp)
│       │   │   └── compress.rs       # 可选的元数据去除 + 压缩
│       │   └── pipeline.rs           # 管线编排: 读取→解码→生成→写回→DB→缓存
│       └── Cargo.toml
├── web-ui/                           # Vite React SPA (独立 npm 项目)
│   ├── src/
│   │   ├── pages/
│   │   │   ├── Login.tsx             # 登录页 (用户名+密码, 记住我)
│   │   │   ├── Register.tsx          # 邀请码注册
│   │   │   ├── Dashboard.tsx         # 上传主页 (拖拽+最近上传)
│   │   │   ├── Gallery.tsx           # 图片库 (网格/列表, 搜索筛选)
│   │   │   ├── ImageDetail.tsx       # 图片详情 (预览+链接复制面板)
│   │   │   ├── Settings.tsx          # 用户设置 (存储后端偏好等)
│   │   │   └── Admin/                # 管理员面板 (用户管理, 系统统计)
│   │   ├── components/
│   │   │   ├── ui/                   # shadcn/ui 基础组件
│   │   │   ├── DropZone.tsx          # 拖拽上传 (react-dropzone)
│   │   │   ├── UploadCard.tsx        # 上传进度卡片
│   │   │   ├── ImageCard.tsx         # 图片缩略图卡片
│   │   │   ├── LinkCopyPanel.tsx     # 链接复制面板
│   │   │   ├── ThemeToggle.tsx       # 主题切换按钮
│   │   │   └── Layout.tsx            # 通用布局 (毛玻璃侧边栏)
│   │   ├── api/
│   │   │   ├── client.ts            # ky 客户端封装 (JWT 自动刷新+限流)
│   │   │   └── keys.ts              # TanStack Query key 工厂
│   │   ├── stores/
│   │   │   ├── auth.ts              # Zustand 认证状态
│   │   │   └── ui.ts                # 全局 UI 状态 (主题/侧边栏)
│   │   └── App.tsx
│   ├── package.json
│   ├── tsconfig.json
│   └── vite.config.ts
├── migrations/
│   ├── 0001_create_users.sql
│   ├── 0002_create_images.sql
│   └── 0003_create_upload_tasks.sql
├── docker-compose.yml                # dev: nginx + api + worker + pg + redis + rustfs
├── docker-compose.prod.yml           # prod: 差异化(SSL, 外部服务, secrets)
├── Dockerfile.api
├── Dockerfile.worker
├── nginx/
│   └── nginx.conf
└── docs/superpowers/specs/
    └── 2026-07-11-pichost-design.md
```

---

## 3. 数据模型

### 3.1 users 表

| 字段 | 类型 | 约束 | 说明 |
|------|------|------|------|
| `id` | `UUID` | PK, DEFAULT gen_random_uuid() | 用户唯一标识 |
| `username` | `VARCHAR(64)` | UNIQUE NOT NULL | 登录用户名 |
| `email` | `VARCHAR(255)` | UNIQUE | 邮箱 (可选) |
| `password_hash` | `VARCHAR(255)` | NOT NULL | Argon2id 哈希值 |
| `storage_backend` | `VARCHAR(32)` | NOT NULL DEFAULT 'local' | `rustfs` 或 `local` |
| `storage_prefix` | `VARCHAR(128)` | NOT NULL DEFAULT 'users/{id}' | 存储路径前缀 |
| `is_admin` | `BOOLEAN` | NOT NULL DEFAULT false | 管理员标志 |
| `created_at` | `TIMESTAMPTZ` | NOT NULL DEFAULT now() | |
| `updated_at` | `TIMESTAMPTZ` | NOT NULL DEFAULT now() | |

### 3.2 images 表

| 字段 | 类型 | 约束 | 说明 |
|------|------|------|------|
| `id` | `UUID` | PK | 图片唯一标识 |
| `user_id` | `UUID` | FK → users(id), NOT NULL | 上传者 |
| `public_key` | `VARCHAR(16)` | UNIQUE NOT NULL | 公开访问短 ID (6-8 字符随机) |
| `original_name` | `VARCHAR(255)` | NOT NULL | 用户提供的原始文件名 |
| `storage_key` | `VARCHAR(512)` | NOT NULL | 存储后端的路径/Key |
| `storage_backend` | `VARCHAR(32)` | NOT NULL | 所属存储后端标识 |
| `mime_type` | `VARCHAR(128)` | NOT NULL | 原始 MIME 类型 |
| `file_size` | `BIGINT` | NOT NULL | 原图字节大小 |
| `width` | `INTEGER` | | 原图宽度 (px) |
| `height` | `INTEGER` | | 原图高度 (px) |
| `sha256` | `VARCHAR(64)` | NOT NULL | 内容 SHA256 哈希 (去重用) |
| `url` | `VARCHAR(1024)` | NOT NULL | 公开访问 URL (`/u/{public_key}`) |
| `thumbnail_key` | `VARCHAR(512)` | | 缩略图存储 Key (Worker 处理后填充) |
| `thumbnail_url` | `VARCHAR(1024)` | | 缩略图公开 URL |
| `webp_key` | `VARCHAR(512)` | | WebP 转换后的存储 Key |
| `webp_url` | `VARCHAR(1024)` | | WebP 公开 URL |
| `status` | `VARCHAR(16)` | NOT NULL DEFAULT 'pending' | `pending` / `processing` / `ready` / `failed` |
| `created_at` | `TIMESTAMPTZ` | NOT NULL DEFAULT now() | |

**唯一索引**: `(user_id, sha256)` — 同一用户上传相同内容的文件时，直接返回既有链接，不重复存储。

### 3.3 upload_tasks 表

| 字段 | 类型 | 约束 | 说明 |
|------|------|------|------|
| `id` | `UUID` | PK | 任务 ID |
| `image_id` | `UUID` | FK → images(id), NOT NULL | 关联图片 |
| `task_type` | `VARCHAR(32)` | NOT NULL | `thumbnail` / `compress` / `convert_webp` / `all` |
| `payload` | `JSONB` | | 任务参数 (目标尺寸、质量等) |
| `status` | `VARCHAR(16)` | NOT NULL DEFAULT 'queued' | `queued` / `processing` / `done` / `failed` |
| `error` | `TEXT` | | 失败原因 |
| `retry_count` | `INTEGER` | NOT NULL DEFAULT 0 | 当前重试次数 |
| `created_at` | `TIMESTAMPTZ` | NOT NULL DEFAULT now() | |
| `completed_at` | `TIMESTAMPTZ` | | |

### 3.4 存储路径策略

```
存储 Key 格式: users/{user_id}/{yyyy}/{mm}/{随机短ID}.{扩展名子}
公开访问 Key:  {6-8 字符随机 base62 short_id}

示例:
  存储 Key:      users/550e8400-e29b/2026/07/k3Xf9a.png
  公开 URL:      /u/k3Xf9a (后端通过 public_key 查 storage_key)
  缩略图 Key:    users/550e8400-e29b/2026/07/k3Xf9a_thumb.jpg
  WebP Key:     users/550e8400-e29b/2026/07/k3Xf9a.webp
```

公开 URL 不暴露用户 ID 结构。后端通过 `public_key` 字段查找对应的 `storage_key` 后再从存储读取。

---

## 4. REST API 设计

所有 API 路径前缀 `/api/v1`，JSON 请求/响应。

### 4.1 认证模块

| Method | Path | 说明 | 认证 |
|--------|------|------|------|
| `POST` | `/auth/register` | 注册 (首次初始化或凭邀请码) | 公开 |
| `POST` | `/auth/login` | 登录，返回 access_token + refresh_token | 公开 |
| `POST` | `/auth/refresh` | 刷新 access token (用 refresh token) | Refresh token |
| `POST` | `/auth/logout` | 登出 (access jti 进黑名单，refresh session 删除) | JWT |

### 4.2 图片管理

| Method | Path | 说明 | 认证 |
|--------|------|------|------|
| `POST` | `/images` | 上传图片 (multipart/form-data, 支持单文件) | JWT |
| `GET` | `/images` | 图片列表 (分页、排序、搜索) | JWT |
| `GET` | `/images/:id` | 单张图片详情 (含所有链接格式和状态) | JWT |
| `DELETE` | `/images/:id` | 删除图片 (同时删除存储文件) | JWT |
| `GET` | `/images/:id/links` | 仅获取所有链接格式 | JWT |

### 4.3 公开访问

| Method | Path | 说明 | 认证 |
|--------|------|------|------|
| `GET` | `/u/:public_key` | 公开访问原始图片 (永久固定 URL) | 公开 |
| `GET` | `/t/:public_key` | 公开访问缩略图 | 公开 |

### 4.4 用户管理

| Method | Path | 说明 | 认证 |
|--------|------|------|------|
| `GET` | `/users/me` | 当前用户信息 | JWT |
| `PATCH` | `/users/me` | 更新自己的设置 (存储后端偏好等) | JWT |
| `GET` | `/users/me/stats` | 使用统计 (上传数量、存储占用等) | JWT |
| `GET` | `/users` | 用户列表 (管理员，普通用户 403) | JWT + admin |
| `PATCH` | `/users/:id` | 修改用户 (管理员) | JWT + admin |

### 4.5 系统

| Method | Path | 说明 | 认证 |
|--------|------|------|------|
| `GET` | `/health` | 服务健康检查 (PG/Redis/Storage 状态) | 公开 |

### 4.6 上传响应示例

```json
POST /api/v1/images
Content-Type: multipart/form-data

Response 201:
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "public_key": "k3Xf9a",
  "original_name": "screenshot.png",
  "url": "https://pichost.example.com/u/k3Xf9a",
  "markdown": "![screenshot.png](https://pichost.example.com/u/k3Xf9a)",
  "html": "<img src=\"https://pichost.example.com/u/k3Xf9a\" alt=\"screenshot.png\" />",
  "bbcode": "[img]https://pichost.example.com/u/k3Xf9a[/img]",
  "sha256": "abc123def456...",
  "file_size": 102400,
  "width": 1920,
  "height": 1080,
  "status": "pending",
  "thumbnail_url": null,
  "webp_url": null,
  "created_at": "2026-07-11T14:30:00Z"
}
```

前端通过 `GET /images/:id` 的 TanStack Query 轮询 (每 2 秒一次)，直到 `status` 变为 `ready`。

---

## 5. 存储抽象

### 5.1 StorageBackend Trait

```rust
// crates/pichost-core/src/storage/mod.rs

#[async_trait::async_trait]
pub trait StorageBackend: Send + Sync {
    /// 保存文件，返回存储 Key
    async fn put(&self, key: &str, data: &[u8], content_type: &str) -> Result<String, StorageError>;

    /// 读取文件，返回文件字节
    async fn get(&self, key: &str) -> Result<Vec<u8>, StorageError>;

    /// 删除文件
    async fn delete(&self, key: &str) -> Result<(), StorageError>;

    /// 生成公开访问 URL
    fn public_url(&self, key: &str) -> String;

    /// 检查文件是否存在
    async fn exists(&self, key: &str) -> Result<bool, StorageError>;

    /// 后端名称 (用于日志、路由)
    fn backend_name(&self) -> &str;
}

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("文件不存在: {0}")]
    NotFound(String),
    #[error("写入失败: {0}")]
    WriteFailed(String),
    #[error("读取失败: {0}")]
    ReadFailed(String),
    #[error("连接失败: {0}")]
    ConnectionFailed(String),
    #[error("配置错误: {0}")]
    Config(String),
}
```

### 5.2 两个实现

**LocalStorage** (`crates/pichost-core/src/storage/local.rs`):
- 写入: `tokio::fs::write(base_path.join(key), data)`
- 读取: `tokio::fs::read(base_path.join(key))`
- 删除: `tokio::fs::remove_file(base_path.join(key))`
- exists: `tokio::fs::try_exists()`
- public_url: `{base_url}/{key}`
- backend_name: `"local"`
- 配置: `base_path` (存储目录), `base_url` (外部访问 URL)

**RustfsStorage** (`crates/pichost-core/src/storage/rustfs.rs`):
- 通过 `aws-sdk-s3` crate 连接 RustFS (S3 兼容协议)
- put/get/delete 使用 S3 PutObject/GetObject/DeleteObject API
- exists 用 HeadObject 检查
- public_url: `{endpoint}/{bucket}/{key}`
- backend_name: `"rustfs"`
- 配置: endpoint, bucket, access_key, secret_key, region, use_ssl

### 5.3 StorageRouter

```rust
// crates/pichost-core/src/storage/router.rs

pub struct StorageRouter {
    backends: HashMap<String, Arc<dyn StorageBackend>>,
    default: String,                         // 默认后端名
}

impl StorageRouter {
    /// 根据用户配置的 storage_backend 字段选择后端
    pub fn for_user(&self, user: &User) -> &Arc<dyn StorageBackend> {
        self.backends.get(&user.storage_backend)
            .unwrap_or(&self.default_backend())
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn StorageBackend>> { ... }
    pub fn default_backend(&self) -> &Arc<dyn StorageBackend> { ... }
}
```

### 5.4 支持的文件格式

```
允许 MIME:
  image/png, image/jpeg, image/gif, image/webp,
  image/svg+xml, image/avif, image/bmp

单文件大小上限:
  - 管理员: 50MB
  - 普通用户: 10MB

Magic bytes 校验: 使用 infer crate 读取前 512 字节判断真实类型,
  防止伪造 Content-Type 的文件上传
拒绝: 非图片 MIME, 超大小, 0 字节文件
```

---

## 6. 图片处理 Worker

### 6.1 Worker 架构

Worker 是 `pichost-worker` crate 编译出的独立二进制进程，通过 Redis 队列消费处理任务。

```
crates/pichost-worker/src/
├── main.rs          # 入口: 解析配置 → 初始化 pool → 启动 BRPOP 循环
├── queue/mod.rs     # Redis队列封装:
│                    #   - enqueue(task): LPUSH tasks:pending
│                    #   - dequeue(): BRPOPLPUSH pending → processing
│                    #   - ack(task_id): LREM processing + HSET task:{id} status=done
│                    #   - nack(task_id, retry): 按策略重推或投死信
│                    #   - recover(): 启动时扫描 processing 超时任务重投
├── processor/
│   ├── mod.rs       # 处理器调度入口
│   ├── thumbnail.rs # 缩略图: 等比缩放长边≤300px, 透明图PNG, 照片JPEG
│   ├── webp.rs      # WebP转换: 质量82, GIF/SVG跳过
│   └── compress.rs  # 原图元数据去除(EXIF), 默认关闭可配开启
└── pipeline.rs      # 管线编排:
                     #   1. StorageBackend.get(source_key) → bytes
                     #   2. image::load_from_memory(bytes) → DynamicImage
                     #   3. processor::thumbnail(&img) → thumb_bytes
                     #   4. processor::webp(&img) → webp_bytes
                     #   5. (可选) compress(&img) → optimized_bytes
                     #   6. StorageBackend.put(thumb_key, thumb_bytes)
                     #   7. StorageBackend.put(webp_key, webp_bytes)
                     #   8. UPDATE images SET thumbnail_key/url, webp_key/url, status='ready'
                     #   9. Redis: DEL img:{id}, DEL thumb:{thumb_key}
```

### 6.2 队列协议

```
Redis 队列结构:

  ╔════════════════════════════════════╗
  ║  pichost:tasks:pending (List)     ║  待处理任务
  ║  pichost:tasks:processing (List)  ║  处理中 (防止崩溃丢失)
  ║  pichost:task:{task_id} (Hash)    ║  任务 payload (JSON)
  ║  pichost:tasks:dead (Set)         ║  死信队列 (重试耗尽)
  ╚════════════════════════════════════╝

任务 Payload (JSON):
{
  "task_id": "uuid-xxx",
  "image_id": "uuid-xxx",
  "user_id": "uuid-xxx",
  "storage_backend": "rustfs",
  "source_key": "users/uuid/2026/07/k3Xf9a.png",
  "source_mime": "image/png",
  "task_type": "all",
  "config": {
    "thumbnail_size": 300,
    "thumbnail_quality": 85,
    "webp_quality": 82,
    "compress_quality": 80
  },
  "retry_count": 0,
  "max_retries": 3,
  "created_at": "2026-07-11T..."
}
```

### 6.3 Worker 主循环

```
Producer (API):
  上传完成 → LPUSH pichost:tasks:pending <task_id>
            → HSET pichost:task:{task_id} <payload_json>

Consumer (Worker):
  for each worker in pool (默认 4 并发):
    1. BRPOPLPUSH pichost:tasks:pending pichost:tasks:processing 5s
    2. (阻塞 5 秒，无任务则重试)
    3. 拿到 task_id → HGETALL pichost:task:{id} → 反序列化为 ProcessTask
    4. 执行 pipeline(task)
    5. 成功:
       - LREM pichost:tasks:processing 0 <task_id>
       - HSET pichost:task:{id} status=done, completed_at=now
    6. 失败:
       - retry_count += 1
       - 若 < max_retries: LPUSH 回 pending 队头 (指数退避 sleep 2^retry_count秒)
       - 若 >= max_retries:
         - LREM processing
         - SADD pichost:tasks:dead <task_id>
         - UPDATE upload_tasks SET status='failed', error=...
         - UPDATE images SET status='failed'
    7. 回到第 2 步
```

### 6.4 崩溃恢复

```
Worker 启动时:
  - LLEN pichost:tasks:processing → 扫描所有 processing 中的 task_id
  - 检查 pichost:task:{id} 中的 created_at
  - 若 created_at + 5min < now (即超时未处理完):
    - LREM pichost:tasks:processing 0 <task_id>
    - LPUSH pichost:tasks:pending <task_id>
  - 周期性扫描 (每 60 秒) 持续清理
```

### 6.5 Worker 配置

```toml
[worker]
concurrency = 4               # 并发处理数 (默认 = CPU 核心数)
queue_poll_timeout = 5        # BRPOP 阻塞超时 (秒)
task_timeout = 300            # 单任务最大处理时长 (秒)
recovery_scan_interval = 60   # 恢复扫描间隔 (秒)

[worker.processing]
thumbnail_size = 300          # 缩略图长边最大值 (px)
thumbnail_quality = 85        # 缩略图 JPEG 质量
webp_quality = 82             # WebP 转换质量
compress_threshold_kb = 500   # 仅原图 > 500KB 做压缩
```

### 6.6 图片格式处理矩阵

| 原图格式 | 缩略图 | WebP | 压缩 |
|----------|--------|------|------|
| PNG (含 alpha) | → PNG thumb | → WebP (q=82) | 去 EXIF |
| PNG (无 alpha) | → JPEG thumb (q=85) | → WebP (q=82) | 去 EXIF |
| JPEG | → JPEG thumb (q=85) | → WebP (q=82) | 去 EXIF |
| WebP | → JPEG thumb (q=85) | 跳过 | 跳过 |
| GIF (动图) | 跳过 (保留原图) | 跳过 (保留原格式) | 跳过 |
| SVG | → raster PNG 预览 300px | 跳过 (矢量格式) | 跳过 |
| AVIF | → JPEG thumb (q=85) | → WebP (q=82) | 跳过 |
| BMP | → JPEG thumb (q=85) | → WebP (q=82) | 跳过 |

### 6.7 使用的 Rust crates (Worker 相关)

```
image = { version = "0.25", features = ["png", "jpeg", "gif", "webp", "avif", "svg"] }
image-webp = "0.1"          # WebP 编解码
infer = "0.16"              # Magic bytes 文件类型检测
redis = { version = "0.27", features = ["tokio-comp", "connection-manager"] }
tokio = { version = "1.52", features = ["full"] }
sha2 = "0.10"               # SHA256 计算
```

---

## 7. 前端设计

### 7.1 技术栈

| 层级 | 选型 | 理由 |
|------|------|------|
| 构建工具 | Vite 6 + SWC | 极快的 HMR 和构建 |
| UI 框架 | React 19 | 最新 Concurrent Features |
| 路由 | React Router v7 | 嵌套路由, 类型安全 |
| 状态管理 | Zustand | 轻量, 无样板代码 |
| 数据获取 | TanStack Query v5 | 缓存, 轮询, 乐观更新 |
| UI 组件 | shadcn/ui (latest canary) | Radix UI + Tailwind |
| 样式 | Tailwind CSS v4 | utility-first |
| 表单 | React Hook Form + Zod | 类型安全验证 |
| 拖拽上传 | react-dropzone | 成熟可靠 |
| Toast | sonner | 轻量通知 |
| 图标 | lucide-react | shadcn 默认图标库 |
| HTTP | ky | 轻量 fetch 封装 |

### 7.2 路由结构

```
/                     → 重定向到 /login 或 /dashboard
/login                → 登录页 (用户名+密码, 记住我)
/register             → 注册页 (邀请码注册)
/dashboard            → 上传主页 (拖拽区域 + 最近上传卡片)
/gallery              → 图片库 (网格/列表, 搜索筛选, 无限滚动)
/gallery/:imageId     → 图片详情 (预览 + 所有链接格式 + 状态)
/settings             → 用户设置 (存储后端偏好)
/settings/account     → 账号信息 (修改密码等)
/admin                → 管理员面板 (用户管理/系统统计)

未登录访问受保护路由 → 重定向到 /login?redirect=...
```

### 7.3 视觉要求

- **毛玻璃效果**: `backdrop-blur` 玻璃质感卡片, 半透明层叠, 圆润阴影
- **主题切换**: 亮色/暗色/跟随系统三档, CSS variables 全局驱动, 切换不刷新
- **审美基线**: 超越 shadcn/ui 默认平淡风格, 现代精致质感
- **实现方式**: Tailwind `dark:` class + `class-variance-authority` + CSS custom properties

### 7.4 核心页面设计

**上传主页 `/dashboard`**

```
┌──────────────────────────────────────────────────────┐
│  PicHost                    [用户名] [设置] [登出]    │
├──────────────────────────────────────────────────────┤
│                                                       │
│  ┌────────────────────────────────────────────────┐  │
│  │                                                 │  │
│  │         拖拽图片到此处上传                        │  │
│  │         或点击选择文件                            │  │
│  │         支持 PNG JPEG GIF WebP SVG AVIF          │  │
│  │         单文件上限 10MB (管理员 50MB)             │  │
│  │                                                 │  │
│  └────────────────────────────────────────────────┘  │
│                                                       │
│  最近上传 (4 张) ──────────────────────────────►       │
│  ┌──────┐ ┌──────┐ ┌──────┐ ┌──────┐                  │
│  │ 缩略  │ │ 缩略  │ │ 处理中 │ │ 缩略  │    ...        │
│  │ ✓    │ │ ✓    │ │ ⏳    │ │ ✓    │                  │
│  └──────┘ └──────┘ └──────┘ └──────┘                  │
└──────────────────────────────────────────────────────┘
```

**图片详情页 `/gallery/:id`**

```
┌──────────────────────────────────────────────────────┐
│  PicHost   /gallery/3f8a...                          │
├──────────────────────────────────────────────────────┤
│                                                       │
│  ┌─────────────┐  ┌──────────────────────────────┐   │
│  │             │  │  原图: 1920x1080, 150KB       │   │
│  │  原图预览    │  │  SHA256: 3a7b...               │   │
│  │  (可缩放)    │  │  格式: PNG                     │   │
│  │             │  │  上传: 2026-07-11 14:30         │   │
│  └─────────────┘  │                                 │   │
│                    │  选择版本:                       │   │
│  ○ 原图           │  ○ 原图  ● 缩略图  ○ WebP       │   │
│  ● 缩略图         │                                 │   │
│  ○ WebP           │  链接格式:                       │   │
│                    │  ┌──────────────────┐ [复制]   │   │
│                    │  │URL...            │           │   │
│                    │  └──────────────────┘           │   │
│                    │  ┌──────────────────┐ [复制]   │   │
│                    │  │![name](URL)      │           │   │
│                    │  └──────────────────┘           │   │
│                    │  ┌──────────────────┐ [复制]   │   │
│                    │  │<img src="URL">   │           │   │
│                    │  └──────────────────┘           │   │
│                    │  ┌──────────────────┐ [复制]   │   │
│                    │  │[img]URL[/img]    │           │   │
│                    │  └──────────────────┘           │   │
│                    └──────────────────────────────┘   │
│                                                       │
│  [删除图片] [下载原图]                                 │
└──────────────────────────────────────────────────────┘
```

### 7.5 核心交互

**上传流线**:
1. 用户拖拽/选择文件 → react-dropzone 触发
2. 文件卡片出现，显示独立进度条
3. 上传完毕 → 卡片切换到"处理中"状态，tanStack Query 每 2 秒轮询
4. 处理完成 → 卡片显示缩略图 + [复制链接] 按钮

**Zustand 状态管理**:

```ts
// stores/auth.ts
interface AuthState {
  user: User | null;
  accessToken: string | null;
  refreshToken: string | null;
  isAuthenticated: boolean;
  login: (creds: LoginCredentials) => Promise<void>;
  logout: () => Promise<void>;
  refresh: () => Promise<void>;
}

// stores/ui.ts
interface UIState {
  sidebarOpen: boolean;
  theme: 'light' | 'dark' | 'system';
  uploadConcurrent: number;    // 同时上传数
  thumbnailViewSize: 'small' | 'medium' | 'large';
}
```

**API 客户端 (ky)**:

```ts
// api/client.ts
const client = ky.create({
  prefixUrl: '/api/v1',
  hooks: {
    beforeRequest: [(req) => {
      const token = useAuthStore.getState().accessToken;
      if (token) req.headers.set('Authorization', `Bearer ${token}`);
    }],
    afterResponse: [async (req, opts, res) => {
      if (res.status === 401) {
        const refreshed = await useAuthStore.getState().refresh();
        if (refreshed) return ky(req);
        useAuthStore.getState().logout();
      }
    }],
    beforeError: [(error) => {
      toast.error(error.response?.message ?? '请求失败');
      return error;
    }],
  },
});
```

**TanStack Query 轮询**:

```ts
function useImage(id: string) {
  return useQuery({
    queryKey: ['images', id],
    queryFn: () => api.getImage(id),
    refetchInterval: (query) =>
      query.state.data?.status === 'ready' ? false : 2000,
  });
}
```

---

## 8. 缓存策略 (Redis 8.0)

### 8.1 完整 Key 结构

| Key 模板 | 类型 | TTL | 说明 |
|----------|------|-----|------|
| `pichost:img:{image_id}` | Hash | 10 min | 图片元数据缓存 (id, user_id, urls, status, 尺寸) |
| `pichost:thumb:{thumbnail_key}` | String (bytes) | 1 hour | 缩略图二进制缓存 (5-50KB) |
| `pichost:user:{user_id}:stats` | Hash | 5 min | 用户统计 (total_images, total_size, quota_used) |
| `pichost:session:{refresh_jti}` | Hash | 30 day | Refresh token 会话信息 (user_id, user_agent, ip, created_at) |
| `pichost:blacklist:{access_jti}` | String ("1") | token 剩余有效期 | Access token 黑名单 |
| `pichost:rate:{user_id}:{endpoint}` | String (counter) | 1 min | 用户级别限流计数器 |
| `pichost:rate:ip:{ip}:{endpoint}` | String (counter) | 1 min | IP 级别限流计数器 |
| `pichost:tasks:pending` | List | - | 待处理任务 ID 列表 |
| `pichost:tasks:processing` | List | - | 处理中任务 ID 列表 |
| `pichost:task:{task_id}` | Hash (JSON) | 24 hours | 任务详细 payload |
| `pichost:tasks:dead` | Set | - | 死信队列 (超过重试上限) |
| `pichost:invite:{code}` | Hash | 7 hours | 邀请码 (created_by, expires_at, used_by) |

### 8.2 缓存读写策略

**元数据缓存 (Cache-Aside)**:
- **读**: 先查 Redis → 命中直接返回 → 未命中查 PostgreSQL → 填充 Redis → 返回
- **写**: 写入 PostgreSQL → 同步 HSET Redis → 返回
- **删**: 删除 PostgreSQL rows → DEL Redis keys → 返回
- **兜底**: TTL 自动过期, maxmemory-policy `allkeys-lru`

**缩略图缓存**:
- 读: 先查 Redis `GET thumb:{key}` → 命中直接返回 bytes → 未命中从 StorageBackend 读取 → 填充 Redis
- 写: Worker 处理完成 → 更新 DB → HSET img:{id} → DEL thumb:{key} (下次访问重建)
- 内存估算: 2 万张活跃图 × 20KB = 400MB, 大多数 Redis 实例轻松承载

**一致性保证**:
- 上传: INSERT DB → HSET Redis (同步) → 查询直接命中 ✅ 强一致
- Worker 处理: UPDATE DB → HSET Redis → DEL thumb (同步) ✅ 强一致
- 删除: DELETE DB → DEL Redis → DEL Storage (因果顺序: 先 DB) ✅ 强一致
- 极端: Redis 故障 → fail-open 降级查 DB, 不阻塞

### 8.3 限流实现

```
登录:   5/min/IP       Redis: INCR pichost:rate:ip:{ip}:login EXPIRE 60
上传:   30/min/用户     Redis: INCR pichost:rate:{user_id}:upload EXPIRE 60
普通API: 60/min/用户    Redis: INCR pichost:rate:{user_id}:general EXPIRE 60
公开图片: 200/min/IP   Redis: INCR pichost:rate:ip:{ip}:public EXPIRE 60

超限: 429 Too Many Requests + Retry-After header
响应头: X-RateLimit-Limit / X-RateLimit-Remaining / X-RateLimit-Reset
```

---

## 9. 认证与安全

### 9.1 注册策略

```
首次启动 (users 表为空):
  - POST /auth/register 对所有 IP 开放
  - 创建第一个用户 → 自动设为 is_admin = true
  - 创建成功后, 注册端点关闭或限制为仅管理员可调用

后续注册:
  - 管理员在后台生成邀请码 (32 字节随机字符串, 一次使用, 7 天有效)
  - POST /auth/register body: { username, password, invite_code }
  - Redis 验证邀请码: HGETALL pichost:invite:{code} → 检查 expired, used_by
  - 使用后: HSET used_by + DEL 7 天后自动过期
```

### 9.2 登录流程

```
POST /auth/login { username, password }
    │
    ▼
SELECT password_hash FROM users WHERE username = ?
    │
    ▼
argon2::verify_encoded(password_hash, password) ?
    │
    ├─ 失败 → 统一 500ms sleep → 401 "用户名或密码错误"
    │         (不透露是用户名错还是密码错)
    │
    └─ 成功
            │
            ▼
       生成 Access Token (JWT):
         - jti: uuid (用于黑名单)
         - sub: user_id (UUID)
         - is_admin: bool
         - iat: now unix timestamp
         - exp: iat + 900 (15 分钟)

       生成 Refresh Token (JWT):
         - jti: uuid
         - sub: user_id
         - iat: now
         - exp: iat + 2592000 (30 天)

       Redis: HSET pichost:session:{refresh_jti}
         user_id, created_at, user_agent, ip
         EXPIRE 2592000

       返回: { access_token, refresh_token, user: { id, username, is_admin } }
```

### 9.3 Token 刷新

```
POST /auth/refresh
Authorization: Bearer {refresh_token}

  1. 验证 refresh_token JWT 签名 + 过期
  2. 检查 Redis EXISTS pichost:session:{jti}
     - 不存在 → 已登出 or 已过期 → 401
  3. 签发新 access_token (新 jti, 新 iat, 新 exp)
  4. (滚动刷新模式) 删除旧 refresh session, 签发新 refresh_token+session
  5. 返回: { access_token, refresh_token (如启用滚动) }
```

### 9.4 登出

```
POST /auth/logout
Authorization: Bearer {access_token}

  1. 解码 JWT 获取 jti 和剩余有效期 (exp - now)
  2. Redis: SETEX pichost:blacklist:{jti} "1" {剩余秒数}
  3. Redis: DEL pichost:session:{refresh_jti}
  4. 返回 200
```

### 9.5 中间件验证流程

```
每个受保护请求:
  1. 提取 Authorization: Bearer {access_token}
  2. 解码 JWT → 验证签名 + exp + iat
  3. Redis: EXISTS pichost:blacklist:{jti} → true = 已登出 → 401
  4. req.extensions().insert(AuthUser { id, is_admin })
  5. next.run(req)
```

### 9.6 安全措施清单

| 层 | 措施 |
|----|------|
| 密码 | Argon2id (mem 19MB, time 2, par 1), 最少 8 字符 |
| 文件上传 | MIME header + infer magic bytes 双重校验 |
| XSS | SVG 禁止外部实体; Content-Type 强制 image/*; Content-Disposition 防止内联执行 |
| CORS | 开发 localhost:5173, 生产白名单 |
| 安全头 | X-Content-Type-Options: nosniff, X-Frame-Options: DENY, CSP, HSTS, Referrer-Policy |
| 暴力破解 | 5 次/15min/IP 失败 → IP 临时封禁 15 分钟; 登录响应固定 500ms 延迟 |
| 目录遍历 | 文件名净化, 移除 `../` 等字符; 存储 Key 使用随机短 ID |
| 旁路遍历 | public_key 随机不可枚举; 不暴露 user_id 在公开 URL |

**Argon2 参数** (来自 `argon2` crate):

```rust
use argon2::{Argon2, PasswordHasher, PasswordVerifier, password_hash::SaltString};

let argon2 = Argon2::default();  // 默认使用 Argon2id + 推荐参数
let salt = SaltString::generate(&mut rng);
let hash = argon2.hash_password(password.as_bytes(), &salt)?.to_string();
```

### 9.7 使用的 crates (认证/安全相关)

```
jsonwebtoken = "9"      # JWT 编解码
argon2 = "0.5"          # 密码哈希
rand = "0.8"            # 随机数生成
infer = "0.16"          # Magic bytes 检测
tower-http = { version = "0.6", features = ["cors", "trace"] }
```

---

## 10. 错误处理与可观测性

### 10.1 错误类型层次

```rust
// crates/pichost-core/src/error.rs

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("认证失败: {0}")]
    Authentication(String),                // → 401

    #[error("权限不足: {0}")]
    Authorization(String),                 // → 403

    #[error("资源不存在: {0}")]
    NotFound(String),                      // → 404

    #[error("验证失败: {0}")]
    Validation(String),                    // → 400

    #[error("上传失败: {0}")]
    Upload(String),                        // → 400

    #[error("请求过多, 请稍后重试")]
    RateLimited,                           // → 429

    #[error("存储失败: {0}")]
    Storage(#[from] StorageError),         // → 404/500

    #[error("数据库错误")]
    Database(#[from] sqlx::Error),         // → 500

    #[error("图片处理失败: {0}")]
    Processing(String),                    // → 500

    #[error("内部错误")]
    Internal,                              // → 500

    #[error("{0}")]
    Business(&'static str),                // → 400/409
}
```

**响应格式**:

```json
// 统一错误响应
{
  "error": "人类可读消息",
  "code": "UPLOAD_FAILED",
  "detail": "详细信息 (仅 400 错误提供, 401/500 不提供)"
}
```

### 10.2 日志策略

```rust
// 使用 tracing crate
use tracing::{info, warn, error, instrument};

#[instrument(skip(state), fields(user_id = %user.id, file_name = %file.name))]
async fn handle_upload(state: &AppState, user: &User, file: MultipartFile) -> Result<ImageResponse> {
    info!("开始上传");
    let result = upload_pipeline(state, user, file).await;
    match &result {
        Ok(img) => info!(image_id = %img.id, size = %img.file_size, "上传成功"),
        Err(e) => error!(error = %e, "上传失败"),
    }
    result
}
```

**日志级别配置**:
```toml
[logging]
level = "info"            # trace, debug, info, warn, error
format = "json"           # 生产 stdout JSON
pretty = false            # 开发可选 true
```

**关键日志节点**:
- API: 所有 HTTP 请求方法/路径/状态码/耗时/IP (tower-http::Trace)
- 上传: 开始→存储完成→DB 完成→入队 各阶段
- Worker: 任务接收→解码→处理→写回→状态更新 各阶段
- 认证: 登录成功/失败 (失败不暴露具体原因)
- 异常: 5xx 错误, DB/Redis 连接失败

### 10.3 健康检查

```
GET /api/health (无认证)

200 OK:
{
  "status": "healthy",
  "components": {
    "postgres": { "status": "ok", "latency_ms": 2 },
    "redis": { "status": "ok", "latency_ms": 1 },
    "storage": { "status": "ok", "backend": "local" },
    "worker_queue": {
      "pending": 3,
      "processing": 1,
      "dead": 0
    }
  },
  "uptime_seconds": 3600,
  "version": "1.0.0"
}

503 Service Unavailable:
{
  "status": "degraded",
  "components": {
    "postgres": { "status": "error", "detail": "connection refused" },
    "redis": { "status": "ok" },
    ...
  }
}
```

### 10.4 使用的 crates (错误/日志相关)

```
thiserror = "2"               # 错误类型派生
tracing = "0.1"               # 结构化日志
tracing-subscriber = "0.3"    # 日志格式化/输出
tower-http = { version = "0.6", features = ["trace"] }
```

---

## 11. 配置管理

### 11.1 配置层级 (figment)

```
优先级 (高→低):
  1. 环境变量 (PICHOST_* 前缀)
  2. .env 文件
  3. config.toml (默认配置文件)
  4. 硬编码默认值
```

### 11.2 完整配置结构

```rust
// crates/pichost-core/src/config.rs

pub struct AppConfig {
    pub server: ServerConfig,
    pub auth: AuthConfig,
    pub storage: StorageConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub worker: WorkerConfig,
    pub upload: UploadConfig,
    pub logging: LoggingConfig,
}

pub struct ServerConfig {
    pub host: String,              // "0.0.0.0"
    pub port: u16,                 // 3000
    pub public_url: String,        // "https://pichost.example.com"
    pub cors_origins: Vec<String>, // ["http://localhost:5173"]
    pub behind_proxy: bool,        // true (信任 X-Forwarded-* headers)
}

pub struct AuthConfig {
    pub jwt_secret: String,        // PICHOST_JWT_SECRET (至少 32 字节)
    pub access_token_ttl: u64,     // 900 (15min)
    pub refresh_token_ttl: u64,    // 2592000 (30d)
    pub refresh_rolling: bool,     // true
    pub invite_only: bool,         // true
    pub argon2_memory_kib: u32,    // 19456
    pub argon2_time: u32,         // 2
    pub argon2_parallelism: u32,   // 1
}

pub struct StorageConfig {
    pub default_backend: String,   // "local"
    pub local: LocalStorageConfig,
    pub rustfs: RustfsStorageConfig,
}

pub struct LocalStorageConfig {
    pub base_path: String,         // "/data/pichost/storage"
    pub base_url: String,          // "{public_url}/u"
}

pub struct RustfsStorageConfig {
    pub endpoint: String,          // "http://rustfs:9000"
    pub bucket: String,            // "pichost"
    pub access_key: String,        // from environment
    pub secret_key: String,        // from environment
    pub region: String,            // "us-east-1"
    pub use_ssl: bool,            // false (内网)
    pub public_endpoint: String,   // "http://rustfs:9000/pichost"
}

pub struct DatabaseConfig {
    pub url: String,               // "postgres://pichost:pichost@postgres/pichost"
    pub max_connections: u32,      // 20
    pub min_connections: u32,      // 5
    pub run_migrations: bool,      // true
}

pub struct RedisConfig {
    pub url: String,               // "redis://redis:6379"
    pub pool_size: u32,            // 20
    pub cache_default_ttl: u64,    // 600 (10min)
    pub thumb_cache_ttl: u64,      // 3600 (1hour)
    pub maxmemory_policy: String,  // "allkeys-lru"
}

pub struct WorkerConfig {
    pub concurrency: usize,        // 4
    pub queue_poll_timeout: u64,   // 5
    pub task_timeout: u64,         // 300
    pub recovery_scan_interval: u64,// 60
    pub thumbnail_size: u32,       // 300
    pub thumbnail_quality: u32,    // 85
    pub webp_quality: u32,        // 82
    pub compress_threshold_kb: u64,// 500
}

pub struct UploadConfig {
    pub max_file_size_admin: u64,  // 52428800 (50MB)
    pub max_file_size_user: u64,   // 10485760 (10MB)
    pub allowed_mimes: Vec<String>,// ["image/png","image/jpeg",...]
    pub max_files_concurrent: u32, // 4
}

pub struct LoggingConfig {
    pub level: String,             // "info"
    pub format: String,           // "json"
    pub pretty: bool,             // false
}
```

### 11.3 环境变量映射

```
PICHOST_SERVER_PORT=3000
PICHOST_JWT_SECRET=xxx
PICHOST_PUBLIC_URL=https://pichost.example.com
PICHOST_STORAGE_DEFAULT_BACKEND=local
PICHOST_RUSTFS_ENDPOINT=http://rustfs:9000
PICHOST_RUSTFS_ACCESS_KEY=xxx
PICHOST_RUSTFS_SECRET_KEY=xxx
PICHOST_RUSTFS_BUCKET=pichost
DATABASE_URL=postgres://pichost:pichost@postgres:5432/pichost
PICHOST_REDIS_URL=redis://redis:6379
PICHOST_LOG_LEVEL=info
```

---

## 12. 部署

### 12.1 Docker Compose 服务清单

```yaml
# docker-compose.yml

services:
  nginx:
    image: nginx:1.27-alpine
    ports: ["8080:80"]
    volumes:
      - ./nginx/nginx.conf:/etc/nginx/nginx.conf:ro
      - ./web-ui/dist:/usr/share/nginx/html:ro
    depends_on: [api]

  api:
    build: { context: ., dockerfile: Dockerfile.api }
    environment:
      - DATABASE_URL=postgres://pichost:pichost@postgres/pichost
      - PICHOST_REDIS_URL=redis://redis:6379
      - PICHOST_JWT_SECRET=${PICHOST_JWT_SECRET}
      - PICHOST_STORAGE_DEFAULT_BACKEND=local
    deploy: { replicas: 2 }
    volumes: ["./storage-local:/data/pichost/storage"]
    depends_on:
      postgres: { condition: service_healthy }
      redis: { condition: service_started }

  worker:
    build: { context: ., dockerfile: Dockerfile.worker }
    environment:
      - DATABASE_URL=postgres://pichost:pichost@postgres/pichost
      - PICHOST_REDIS_URL=redis://redis:6379
      - PICHOST_JWT_SECRET=${PICHOST_JWT_SECRET}
      - PICHOST_STORAGE_DEFAULT_BACKEND=local
    deploy: { replicas: 2 }
    depends_on:
      postgres: { condition: service_healthy }
      redis: { condition: service_started }

  postgres:
    image: postgres:18-alpine
    environment:
      POSTGRES_USER: pichost
      POSTGRES_PASSWORD: pichost
      POSTGRES_DB: pichost
    volumes: ["pgdata:/var/lib/postgresql/data"]
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U pichost"]
      interval: 5s; timeout: 5s; retries: 5

  redis:
    image: redis:8-alpine
    command: redis-server --maxmemory 512mb --maxmemory-policy allkeys-lru
    volumes: ["redisdata:/data"]

  rustfs:
    image: rustfs/rustfs:latest
    environment:
      RUSTFS_ACCESS_KEY: ${RUSTFS_ACCESS_KEY}
      RUSTFS_SECRET_KEY: ${RUSTFS_SECRET_KEY}
    ports: ["9000:9000"]
    volumes: ["rustfsdata:/data"]

volumes:
  pgdata:
  redisdata:
  rustfsdata:
```

### 12.2 Dockerfile

```dockerfile
# Dockerfile.api
FROM rust:1.96-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --release -p pichost-api

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/pichost-api /usr/local/bin/
EXPOSE 3000
CMD ["pichost-api"]

# Dockerfile.worker (与 api 类似, 只是 build target 不同)
FROM rust:1.96-slim AS builder
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
RUN cargo build --release -p pichost-worker

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /app/target/release/pichost-worker /usr/local/bin/
CMD ["pichost-worker"]
```

### 12.3 Nginx 配置

```nginx
# nginx/nginx.conf
upstream api {
    server api:3000;
}

server {
    listen 80;

    location / {
        root /usr/share/nginx/html;
        try_files $uri $uri/ /index.html;
    }

    location /u/ {
        proxy_pass http://api;
        proxy_cache public_cache;
        proxy_cache_valid 200 1h;
        proxy_cache_key "$uri";
    }

    location /t/ {
        proxy_pass http://api;
        proxy_cache thumb_cache;
        proxy_cache_valid 200 1h;
        proxy_cache_key "$uri";
    }

    location /api/ {
        proxy_pass http://api;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}
```

### 12.4 版本基线汇总

| 组件 | 版本 | 说明 |
|------|------|------|
| Rust | 1.96 | 2026-07 stable |
| PostgreSQL | 18 | 最新 major |
| Redis | 8.0 | 最新 stable |
| Tokio | 1.52.x | 异步运行时 |
| Axum | 0.8 | Web 框架 |
| nginx | 1.27 | mainline |
| Node.js | 22 LTS | 前端构建 |

### 12.5 本地开发模式

```
仅启动基础设施:
  docker compose up postgres redis rustfs -d

后端开发:
  cd crates/pichost-api && cargo watch -x run

Worker 开发:
  cd crates/pichost-worker && cargo watch -x run

前端开发:
  cd web-ui && npm run dev   (Vite dev server, HMR)
```

### 12.6 Rust crates 依赖清单

```
pichost-core (lib):
  async-trait      - async trait 方法
  aws-sdk-s3       - RustFS S3 客户端
  aws-config       - S3 配置
  redis (0.27)     - Redis 客户端
  deadpool-redis   - 连接池管理
  tokio (1.52)     - 异步运行时
  sqlx (0.8)       - PostgreSQL 驱动
  figment (0.11)   - 多层配置
  serde + serde_json - 序列化
  uuid             - UUID v4/v7
  chrono           - 时间处理
  thiserror (2)    - 错误派生
  tracing          - 结构化日志
  sha2             - SHA256 哈希

pichost-api (bin, 除 core 外):
  axum (0.8)       - Web 框架
  tower            - 中间件基础
  tower-http (0.6)  - CORS, trace, security headers
  jsonwebtoken (9) - JWT 编解码
  argon2 (0.5)     - 密码哈希
  infer            - magic bytes

pichost-worker (bin, 除 core 外):
  image (0.25)     - 图片解码/缩放/编码
  image-webp       - WebP 编解码
  tempfile (dev)   - 测试临时目录

dev/test:
  testcontainers    - 集成测试 Docker 容器
  reqwest           - HTTP 客户端 (集成测试)
  tempfile          - 临时目录
  tokio-test        - tokio 测试工具
```

---

## 13. 测试策略

### 13.1 测试金字塔

```
           ┌───────┐
           │  E2E  │  Playwright (浏览器上完整的用户旅程)
           ├───────┤
           │ 集成  │  testcontainers-rs (API + DB + Redis + Storage)
      ┌────┴───────┴────┐
      │     单元测试     │  cargo test (每个 crate 独立, mock 外部)
      └─────────────────┘
```

### 13.2 测试覆盖

| 层级 | 工具 | 范围 | 用例示例 |
|------|------|------|----------|
| 单元 | `cargo test --lib` | models 序列化, storage trait, config 解析, error 派生 | `test_localstorage_pug_and_get()`, `test_config_env_priority()` |
| 集成 | `cargo test --tests --features integration` | API 路由 + DB + Redis + Storage 真实交互 | `test_login_success()`, `test_upload_detects_duplicate()`, `test_protected_route_401()` |
| Worker | testcontainers + Redis real instance | 队列消费, 图片处理, 重试, 死信 | `test_worker_generates_thumbnail()`, `test_worker_retries_and_dead_letter()` |
| E2E | Playwright | 浏览器端完整流程 | 登录→上传→查看→复制链接→删除 |
| 前端单元 | Vitest + Testing Library | 组件渲染, 状态逻辑, 表单验证 | `test_upload_card_shows_progress()`, `test_copy_button_writes_to_clipboard()` |

### 13.3 集成测试示例

```rust
// crates/pichost-api/tests/auth_test.rs

#[tokio::test]
async fn test_login_success() {
    let (client, _containers) = setup_test_app().await;

    // 先注册
    client.post("/api/v1/auth/register")
        .json(&json!({"username":"alice","password":"password123"}))
        .send().await.expect_status(201);

    // 登录
    let resp = client.post("/api/v1/auth/login")
        .json(&json!({"username":"alice","password":"password123"}))
        .send().await;

    assert_eq!(resp.status(), 200);
    let body: Value = resp.json().await;
    assert!(body["access_token"].is_string());
    assert!(body["refresh_token"].is_string());
    assert!(body["user"]["username"].as_str() == Some("alice"));
}

#[tokio::test]
async fn test_login_fails_with_wrong_password() {
    let (client, _containers) = setup_test_app().await;
    // register...
    let resp = client.post("/api/v1/auth/login")
        .json(&json!({"username":"alice","password":"wrongpass"}))
        .send().await;
    assert_eq!(resp.status(), 401);
}

#[tokio::test]
async fn test_upload_image_success() {
    let (client, _containers) = setup_with_auth().await;
    let token = login_and_get_token(&client, "alice").await;
    let png_bytes = create_valid_png(100, 100);

    let resp = client.post("/api/v1/images").bearer_auth(token)
        .multipart(Form::new().part("file", Part::bytes(png_bytes)
            .file_name("test.png").mime_str("image/png").unwrap()))
        .send().await;

    assert_eq!(resp.status(), 201);
    let body: Value = resp.json().await;
    assert!(body["url"].is_string());
    assert!(body["markdown"].as_str().unwrap().contains("![test.png]"));
    assert!(body["status"] == "pending");
}

#[tokio::test]
async fn test_upload_rejects_non_image() {
    let (client, _containers) = setup_with_auth().await;
    let token = login_and_get_token(&client, "alice").await;

    let resp = client.post("/api/v1/images").bearer_auth(token)
        .multipart(Form::new().part("file", Part::bytes(b"not-an-image")
            .file_name("fake.png").mime_str("image/png").unwrap()))
        .send().await;

    assert_eq!(resp.status(), 400);
}
```

### 13.4 CI Pipeline

```bash
# CI (GitHub Actions / other)

cargo fmt --check                        # 格式
cargo clippy --workspace -- -D warnings  # Lint
cargo check --workspace                  # 编译检查

cargo test --workspace --lib              # 单元测试

docker compose -f docker-compose.test.yml up -d  # 启动 test 基础设施
cargo test --workspace --all-features    # 集成测试
docker compose -f docker-compose.test.yml down

cd web-ui && npm ci && npm test          # 前端单元
cd web-ui && npm run build               # 打包验证
# cd web-ui && npx playwright test        # E2E (CI 环境)
```

---

## 14. 全局架构决策记录 (ADR)

| # | 决策 | 理由 |
|---|------|------|
| 1 | 单体后端核心 + 异步 Worker | 规模匹配 (500-2000 并发), 图片处理不阻塞 API 响应。Worker 独立水平扩展 |
| 2 | StorageBackend trait 抽象 | LocalFS 和 RustFS 共享接口，通过配置切换或并行使用。未来可加 S3/七牛云无需改动核心逻辑 |
| 3 | Redis 兼任队列 + 缓存 + 限流 | 个人/团队自用场景不需要独立的 RabbitMQ/Kafka。Redis 足够可靠且简化部署 |
| 4 | 公开 URL 固定且不暴露 user_id | 通过 public_key 映射存储路径，外链可靠且用户结构不被旁路枚举 |
| 5 | sqlx compile-time checked | 类型安全优于 runtime ORM；性能接近 raw queries；无需 migration 工具外的额外维护 |
| 6 | 3 workspace crates (core/api/worker) | core 避免代码重复 (models/config/error/storage)，api 和 worker 各自独立构建和扩缩 |
| 7 | web-ui 独立 npm 项目 | 前后端解耦，构建产物由 Nginx 直接 serve，api 更新不需要整体重启 |
| 8 | 毛玻璃效果 + 主题切换 | 用户明确要求的视觉方向。Tailwind backdrop-blur + CSS variables 实现 |
| 9 | JWT short access (15min) + long refresh (30d) | 安全性 (短 access 减少被盗风险) 与用户体验 (长 refresh 避免频繁登录) 平衡 |
| 10 | SHA256 内容去重 | 同一用户上传相同文件直接返回既有链接，节省存储和带宽 |

---

## 15. 开发优先级

**P0 — Baseline (必须先完成)**:
- 用户注册/登录/登出 (JWT + Argon2id)
- 图片单文件上传 (LocalFS 存储)
- 公开 URL 访问 + 全格式链接 (URL/Markdown/HTML/BBCode)
- 多用户独立空间
- SHA256 去重检查
- 安全基础: magic bytes 校验, CORS, 限流
- 前端核心页: 登录 + 上传 + 图片详情

**P1 — 增强 (紧随其后)**:
- 缩略图生成 + WebP 转换 (异步 Worker + Redis 队列)
- RustFS 存储后端支持 (aws-sdk-s3 实现)
- Redis 三层缓存 (元数据 + 缩略图 + 用户统计)
- 水平扩展 (Nginx upstream 多 api 实例)
- 图片库浏览 (分页/搜索/排序/无限滚动)
- 管理员面板 (用户管理/系统统计)
- 健康检查 + 结构化日志 (tracing)
- 视觉打磨: 毛玻璃效果 + 亮暗主题切换

**P2 — 完善 (后续迭代)**:
- 多文件并发拖拽上传
- 批量管理 (删除/标签/搜索)
- 邀请码注册系统
- 用户存储配额
- /metrics Prometheus 端点
- CDN 集成 (Cloudflare/七牛云)
