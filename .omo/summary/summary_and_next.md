# PicHost 项目进度

## 图片详情页缩放查看器 ✅ (本次完成)
- **useImageZoom hook**: 纯 zoom/pan 状态管理 (scale/offset, 锚点缩放数学, 范围/平移钳制), 9 个 TDD 单元测试
- **ImageViewer 组件**: 全屏灯箱 (useOverlay 遮罩 + Pointer Events 手势 — 滚轮/拖拽/双击/双指缩放 + 玻璃质感工具栏), 9 个测试
- **i18n**: 新增 6 个翻译键 (en/zh-CN)
- **E2E**: 新增 2 条 image-detail 测试 (e2e/specs/image-detail.spec.ts)
- **版本**: 0.19.1 → 0.20.0
- **验证**: `npx vitest run` ✅, `npm run build` ✅, `npx playwright test e2e/specs/image-detail.spec.ts` ✅

## 当前项目涉及特性

参考 `docs/superpowers/specs/2026-07-11-pichost-design.md`，PicHost 是一个面向个人/团队自用的图床系统。

### P0 核心功能 (已完成)
- 用户注册/登录 (Argon2id)
- JWT 认证 (access + refresh token)
- Redis token 黑名单登出
- 图片上传 + magic byte 校验 + SHA256 去重
- 公开图片服务 `/u/{public_key}`
- 全格式链接 (URL/Markdown/HTML/BBCode)
- 仪表盘 + 画廊 + 图片详情
- 文件大小限制 (50MB admin / 10MB user)
- Docker Compose 部署

### P1 基础设施 (已完成)
- WorkerConfig 配置 + DB 迁移
- RustFS (S3 兼容) 存储后端
- StorageRouter 多后端路由
- 3 层 Redis 缓存 (metadata/thumbnail/stats)
- 健康检查端点 `/api/health`
- 安全响应头 (CSP/HSTS/X-Frame-Options)
- Redis 限流 (4 策略)
- JWT claims 拆分 + token 轮转刷新
- DELETE /images/{id} 图片删除
- Async Worker 缩略图/WebP 处理
- 上传自动入队 Worker

### Plan B: 视觉打磨 + 管理后台 ✅ (本次完成)
- **管理员后端 API**: require_admin 中间件, 4 个端点 (stats, list users, update user, delete user)
- **主题系统**: CSS 变量 (light/dark), Tailwind v4 暗色模式, Zustand theme store, flash prevention script, ThemeToggle 组件
- **毛玻璃视觉打磨**: Layout 组件, 所有页面玻璃质感 (Login/Dashboard/Gallery/ImageDetail/NavBar/DropZone/LinkCard), Button/Input 组件提取
- **管理后台前端**: AdminRoute 守卫, 统计仪表盘 (4 卡片 + 存储后端分解), 用户管理表 (编辑/删除对话框)

## 本次开发完成

### P2: 邀请码注册系统 ✅ (本次完成)
- **Redis 邀请码引擎**: `create_invite_code`, `verify_invite_code`, `consume_invite_code`, `list_invite_codes` 四个方法
- **注册流程改造**: 首个用户自动设为管理员且免邀请码，后续注册需验证邀请码
- **管理后台 API**: `POST/GET /api/v1/admin/invites` 邀请码创建/列表
- **前端注册表单**: 注册模式增加邀请码输入框
- **管理后台页面**: "Invites" 标签页，支持创建/复制/列表邀请码
- **验证**: `cargo clippy --workspace -D warnings` ✅, `cargo test --workspace` ✅(10 passed), `npm run build` ✅

### P2: 图片库增强 ✅ (本次完成)
- **后端**: `ImageListQuery`/`ImageListResponse` 类型, `GET /images` 支持 `page`/`per_page`/`sort`/`order`/`search` 参数, sort 白名单防注入, `idx_images_user_filename` 索引加速文件名搜索
- **前端**: `PaginatedListParams`/`PaginatedResponse<T>` 类型, `SearchBar` (300ms debounce), `SortDropdown` (日期/大小/文件名 + asc/desc), Gallery 使用 `useInfiniteQuery` + IntersectionObserver 无限滚动, `keepPreviousData` 防滤镜切换闪烁
- **验证**: `cargo clippy` ✅, `cargo test` ✅(14 pass), `npm run build` ✅

### P2: 用户存储配额 ✅ (本次完成)
- **数据库**: `storage_quota BIGINT NULL` 列 (NULL = 无限制), 新用户默认 1 GB
- **后端**: `AuthUser`/`UserInfo`/`UserStats` 携带 quota, `process_upload` 配额检查 (413 + quota_bytes/used_bytes/file_bytes), admin 可读写 quota
- **前端**: Dashboard 用量条 (绿/黄/红三级), Admin 用户编辑对话框 quota 输入

### P2: Prometheus /metrics ✅ (本次完成)
- **Backend**: `prometheus` + `lazy_static` crates, metrics registry (CounterVec, HistogramVec, Counter, IntGauge), HTTP middleware tracking all requests, `GET /metrics` public endpoint (Prometheus text format)
- **Business gauges**: uploads_total, upload_errors_total, users_total, images_total, storage_bytes_total

### P2: OAuth 登录 ✅ (本次完成)
- **Backend**: `oauth_accounts` 表, `oauth2` + `reqwest` crates, `GET /auth/oauth/{github,google}` redirect + callback, `POST /users/oauth/link` 账户关联
- **Frontend**: Login 页 GitHub/Google 社交登录按钮, Settings 页 OAuth 关联入口
- **验证**: `cargo clippy` ✅, `cargo test` ✅(14 pass), `npm run build` ✅

### P2: CDN + 水平扩展 ✅ (本次完成 — P2 全部完成!)
- **Nginx**: reverse proxy, proxy_cache (IMAGE_CACHE 50MB/1h), gzip, upstream least_conn
- **Docker**: Nginx 入口 (port 80), API ×2 replicas, Worker ×2 replicas
- **Docs**: CDN 集成指南 (Cloudflare + 其他 CDN)
- **验证**: `cargo clippy` ✅, `cargo test` ✅(14 pass), `npm run build` ✅

## P2 完成总结

所有 10 个 P2 特性全部完成:
1. ✅ 邀请码注册
2. ✅ 图片库增强 (分页/搜索/排序/滚动)
3. ✅ 多文件并发上传
4. ✅ 用户存储配额
5. ✅ 批量管理
6. ✅ /metrics Prometheus
7. ✅ OAuth 登录
8. ✅ CDN 集成
9. ✅ 水平扩展
10. ✅ (plan docs + guides)

版本: `0.14.0` — PicHost P2 阶段完成 🎉

## P4-A: Git 存储后端 + 多后端上传选择 ✅ (本次完成)

参考 `docs/superpowers/specs/2026-07-19-pichost-p4-design.md` §2。

### Git 存储后端
- **GitStorage**: 单一 `StorageBackend` trait 实现，通过 `GitProvider` 枚举区分 GitHub 和 GitCode
- **API 直写**: 通过 GitHub/GitCode Contents REST API 操作文件，不走 clone-commit-push
- **文件路径**: `{prefix}/{YYYY}/{MM}/{DD}/{key}.{ext}`，日期取自服务端时钟，扩展名从 MIME 推导
- **速率限制**: GitHub 5,000/h、GitCode 400/min，429 时返回 retry-after
- **大小限制**: GitCode 超 20MB 返回 413，GitHub 100MB 上限（PicHost 本身 50MB 上限）
- **Token 加密**: AES-256-GCM 加密存储，独立密钥 `PICHOST_AUTH_TOKEN_ENCRYPTION_KEY`

### 存储配置管理
- **数据库**: `user_storage_configs` 表（`0008` 迁移），`images.storage_config_id` 外键
- **Rust 模型**: `UserStorageConfig`、`GitConfigDetail`、`UserStorageConfigResponse`
- **API**: 6 个 CRUD 端点 (`/api/v1/users/me/storage-configs`)，含仓库可达性验证、409 删除保护、Token 掩码返回
- **配置上限**: `PICHOST_STORAGE_MAX_USER_CONFIGS` 可配（默认 5）

### StorageRouter 改造
- `RwLock<HashMap>` 替代 `HashMap`，支持动态注册 Git 后端
- `for_config()` 按配置 ID 路由，`get_or_create_git()` 按需创建+缓存，`evict()` 清理过期

### 多后端上传
- **管线**: `process_upload()` 接收 `storage_config_ids`，循环写入每个后端，每个后端各生成一条 `images` 记录
- **去重**: 扩展为 `(user_id, sha256, storage_config_id)` 三元组
- **双后端并行**: `tokio::join!` 并行写入
- **约束**: 最多 2 个后端，至少 1 个为 `local`

### Gallery 过滤
- `?storage_config_id=uuid` 查询参数，注入 `fetch_user_images`/`count_user_images` SQL
- 前端 Gallery 筛选栏新增后端下拉，图片卡片右上角 provider 图标

### Worker 适配
- `TaskPayload` 扩展 `storage_config_id` + `storage_backend_name`
- `resolve_backend()` 优先使用 `for_config()` 路由 Git 后端

### 前端
- **Settings**: `StorageConfigSection` 组件，表单创建/编辑/删除/设为默认，provider 图标
- **Dashboard**: DropZone 上方多后端选择器（2 个下拉，互斥，最多 2 个）
- **UploadCard**: 显示后端名称
- **Gallery**: 后端过滤下拉 + 图片卡片 provider 标识

### 验证
- `cargo clippy --workspace -D warnings` ✅
- `cargo test --workspace` ✅ (18 pass, 10 ignored)
- `npx tsc --noEmit` ✅
- `npm run build` ✅
- 版本: `0.14.0` → **`0.15.0`**

## P4-B: 剪贴板粘贴 + URL 上传 ✅ (本次完成)

参考 `docs/superpowers/specs/2026-07-19-pichost-p4-design.md` §3。

### 剪贴板粘贴
- **`useClipboardPaste` hook**: 监听 `document` 上的 `paste` 事件，从 `ClipboardItem` 提取图片 `Blob` → `File`
- 集成到 `useUploadQueue.addFiles()`，复用现有上传流程（含多后端选择）

### URL 上传
- **`POST /api/v1/images/upload-url`** 端点：JSON body `{ url, storage_config_ids? }`
- **SSRF 防护**: scheme 白名单 (http/https)，DNS 解析 + 私有 IP 拦截（IPv4 全部保留段 + IPv6 loopback/link-local/unique-local），重定向限制 (5)，超时 (30s)，大小上限 (50MB)，magic byte 校验
- **`fetch_image_from_url()`** 服务函数：下载 → 校验 → 返回 `(bytes, filename)`
- 复用 `process_upload()` 管线，不做重复实现

### 前端
- **`UrlUploadInput`** 组件：URL 输入框 + 上传按钮，置于 DropZone 下方
- **`uploadFromUrl()`** API 客户端：`POST images/upload-url`
- Dashboard 集成：Cmd+V 粘贴 → 加入上传队列；URL 输入 → 服务端下载 → 刷新图库

### 验证
- `cargo clippy --workspace -D warnings` ✅
- `cargo test --workspace` ✅ (29 pass, 10 ignored)
- `npm run build` ✅
- 版本: `0.15.0` → **`0.15.1`**

## P4-C: 图库分类/目录 ✅ (本次完成)

参考 `docs/superpowers/specs/2026-07-19-pichost-p4-design.md` §4。

### 分类系统
- **数据库**: `categories` 表（`0009` 迁移）— 自引用 `parent_id`，应用层强制最大深度 2 级
- **Rust 模型**: `Category` 结构体 (`sqlx::FromRow`)，`Image` 新增 `category_id: Option<Uuid>`
- **API 端点**: 5 个分类 CRUD (`/api/v1/categories`)，树结构返回，`parent_id` 深度校验
- **图片移动**: `POST /api/v1/images/:id/move` + `POST /api/v1/images/batch-move` — 分类所有权校验
- **Gallery 过滤**: `GET /api/v1/images?category_id=uuid` — 新增 `ImageListQuery` 参数 + SQL WHERE 注入

### 前端
- **CategoryTree**: 侧栏树形组件 — 展开/折叠、选中高亮、右键菜单（重命名/删除）
- **CRUD 弹窗**: 创建分类对话框（名称输入 + 回车保存）、内联重命名、删除确认对话框
- **TanStack Query**: `useQuery(['categories'])` 树数据获取，`useMutation` 创建/更新/删除 + `invalidateQueries`
- **Gallery 改造**: 双栏布局 — 左侧 256px 侧栏 + 右侧网格，`category_id` 同步到 URL searchParams
- **ImageDetail**: 分类下拉选择器 — 缩进显示树结构，`moveImageToCategory` mutation

### 验证
- `cargo clippy --workspace -D warnings` ✅
- `cargo test --workspace` ✅ (38 pass, 10 ignored)
- `npm run build` ✅
- 版本: `0.15.1` → **`0.16.0`**

### P4-C 分类内联 CRUD 完成 ✅ (本次完成)
- **Context Menu**: 右键菜单 — Rename/Delete 操作入口，点击外部自动关闭
- **内联重命名**: 点击 Rename → 节点文字变为 input 输入框，Enter 保存、Escape/失焦取消
- **创建模态框**: 标题输入 + 回车/按钮创建，支持 `parent_id`（从侧栏新建时自动设为根级）
- **删除确认**: 二次确认对话框，提示级联删除子分类和图片自动取消关联
- **简化接口**: CategoryTree 移除 `onAddCategory`/`onEditCategory`/`onDeleteCategory` props，Gallery.tsx 删除对应的 stub handler 和旧模态框
- **验证**: `cargo clippy` ✅, `cargo test` ✅(38 pass), `npm run build` ✅

## T4: 添加 imageproc/rusttype 依赖 + 字体嵌入模块 ✅ (本次完成)

参考 `docs/superpowers/specs/2026-07-19-pichost-p4-design.md` §5 (水印设计)。

### 变更内容
- **`pichost-worker/Cargo.toml`**: 新增 `imageproc = "0.25"`、`rusttype = "0.9"` 依赖
- **`pichost-worker/src/fonts.rs`**: 新建字体嵌入模块，包含：
  - `load_font(name)` — 通过名称加载 5 个内嵌 TTF 字体
  - `builtin_font_names()` — 返回可用字体名称列表
  - `scaled_font_size()` — 根据图片对角线计算缩放字号
- **`pichost-worker/src/main.rs`**: 新增 `mod fonts;` 声明
- **字体文件**: `pichost-worker/fonts/` 下 5 个 TTF 文件（NotoSansSC-Regular、NotoSans-Regular、Arial、DejaVuSans、FiraCode-Regular）

### 验证
- `cargo check -p pichost-worker` ✅
- `cargo test -p pichost-worker` ✅ (4 tests: 加载每个内置字体、未知字体报错、DejaVuSans 布局、缩放计算)
- `cargo clippy --workspace -D warnings` ✅

### 注意
- `dead_code` 抑制：T4 导出的函数供后续水印处理任务（T1–T3）使用，当前无消费方
- imageproc 0.25 + rusttype 0.9 + ab_glyph 0.2 版本锁定，未来升级需注意 API 兼容性

## P4-D: 服务端图片水印 ✅ (本次完成)

参考 `docs/superpowers/specs/2026-07-19-pichost-p4-design.md` §5。

### 变更内容

**数据库**:
- 迁移 `0010`: `ALTER TABLE users ADD COLUMN watermark_config JSONB` — 每用户水印配置

**核心模型 (pichost-core)**:
- `WatermarkConfig` 结构体 (10 字段: enabled, text, font, font_size, color, rotation, scale, position, margin_x, margin_y) — 所有字段有 `serde(default)` 默认值
- `WatermarkPosition` 枚举 (6 变体: TopLeft, TopRight, BottomLeft, BottomRight, Center, Tile)
- `User`/`UserProfile`/`UpdateProfileRequest` 新增 `watermark_config` 字段
- `Option<Option<T>>` 反序列化模式: absent=不改, null=清除, value=设置

**API 处理程序 (pichost-api)**:
- `PATCH /users/me` 接受 `watermark_config` — CASE WHEN 模式区分 absent/null/value
- `PATCH /admin/users/:id` 接受 `watermark_config` — fetch_and_merge 模式
- `AuthUser` 中间件读取 watermark_config

**Worker 管线 (pichost-worker)**:
- 字体嵌入: 5 个 TTF 字体 (`fonts.rs`) 通过 `include_bytes!` 编译进二进制
- 水印渲染: `watermark.rs` — `apply_watermark()` 支持所有位置 + RGBA 颜色 + Tile 模式
- 管线集成: `process_task()` 在 `read_source_image` 之后、`process_image_variants` 之前应用水印
- `PipelineError::Watermark` 新错误变体

**前端 (web-ui)**:
- `WatermarkConfig` TypeScript 类型 + `UserProfile`/`UpdateProfileRequest` 更新
- `WatermarkSettings` 组件 — 开关、文字/字体/颜色/位置/旋转/缩放/边距控件
- 集成到 Settings 页面 (Profile → Storage → Watermark → OAuth 顺序)

### 依赖
- `imageproc = "0.25"` — 图片绘制 (draw_text_mut)
- `rusttype = "0.9"` — TTF 字体解析
- `ab_glyph = "0.2"` — imageproc 0.25 内部使用 ab_glyph

### 验证
- `cargo clippy --workspace -D warnings` ✅
- `cargo test --workspace` ✅ (63 pass, 10 ignored — 新增 25 测试：7 config 测试 + 4 字体测试 + 14 水印渲染测试)
- `npm run build` ✅

### 版本: 0.16.0 → **0.16.1**

## P4-E: 客户端图片预处理 ✅ (本次完成)

参考 `docs/superpowers/specs/2026-07-19-pichost-p4-design.md` §6。

### 客户端图片预处理
- **Web Worker 管线**: `OffscreenCanvas` 实现浏览器端图片处理（EXIF 移除、缩放、格式转换、压缩、旋转），不阻塞主线程
- **预处理操作**: EXIF 移除（JPEG 二进制）、缩放、格式转换（WebP/JPEG/PNG）、压缩、旋转（0°/90°/180°/270°）
- **Zustand Store**: `types/preprocessing.ts` + `stores/preprocessing.ts` — localStorage 持久化（遵循 `ui.ts` 主题模式）
- **设置界面**: `PreprocessingSettings` — 5 个操作的开关和控件，集成到 Settings 页面
- **Dashboard 状态栏**: `PreprocessingStatus` — 显示当前启用的预处理设置
- **UploadCard**: 新增 "Processing..." 状态
- **降级方案**: OffscreenCanvas 不可用时回退到主线程

### 验证
- `cargo clippy --workspace -D warnings` ✅
- `cargo test --workspace` ✅ (63 pass, 10 ignored)
- `npm run build` ✅
- `npx vitest run` ✅ (20/20 pass — 8 store + 12 processor)

### 版本: 0.16.1 → **0.16.2**

## P4-F: 文件名保留 + 重命名 ✅ (本次完成)

参考 `docs/superpowers/specs/2026-07-19-pichost-p4-design.md` §7。

### 变更内容
- **后端**: `PATCH /api/v1/images/:id` — 重命名图片显示名称，校验（≤255 字符、无路径分隔符/空字节），Redis 缓存失效
- **前端**: ImageDetail 页面内联重命名 — 点击文件名 → input 输入框，Enter 保存、Escape/失焦取消，Pencil 图标悬停提示
- 5 个单元测试验证校验逻辑

### 验证
- `cargo clippy --workspace -D warnings` ✅
- `cargo test --workspace` ✅ (68 pass, 10 ignored)
- `npm run build` ✅

### 版本: 0.16.2 → **0.16.3**

## P4-G/H/I: Settings UI + Packaging + System Config ✅ (本次完成)

### P4-G: Settings Entry Optimization
- **NavBar**: 用户区改为 `DropdownMenu`（Settings/Admin/Logout），新建 `web-ui/src/components/ui/DropdownMenu.tsx`
- **Settings 页面**: 重构为手风琴分区（Profile/Password/Storage Usage/Storage Backends/Watermark/Preprocessing/OAuth），支持 hash 自动展开（`#settings?section=...`）
- **设计系统**: theme.css 新增 `--color-surface-hover` token
- 无后端变更

### P4-H: Software Packaging + CI/CD
- **systemd 服务**: `scripts/pichost-api.service` + `scripts/pichost-worker.service`（Type=simple, User=pichost, WorkingDirectory=/opt/pichost, EnvironmentFile=/etc/pichost/.env）
- **安装/卸载脚本**: `scripts/install.sh` / `scripts/uninstall.sh`
- **Release CI**: `.github/workflows/release.yml` — `v*` tag 触发，构建 x86_64-unknown-linux-gnu，cargo test + clippy，打包 `.tar.gz`（binaries/web-ui/migrations/nginx/scripts）
- **env.example**: 补齐全部 `PICHOST_*` 变量（DATABASE_URL/REDIS_URL/STORAGE/AUTH）
- 无 Rust 代码变更

### P4-I: System Configuration Management
- **Config 服务**: `pichost-api/src/services/config.rs` — config.toml 读写（figment 兼容嵌套键 `[database] url`/`[redis] url`/`[server] public_url`/`[storage] default_backend`/`local_base_path`），带时间戳 `.bak` 备份/恢复，`test_database_connection`（5s 超时）/`test_redis_connection`
- **依赖**: pichost-api 新增 `toml_edit = "0.22"`、`regex = "1"`、`thiserror.workspace = true`、`tempfile = "3"`（dev-dep）
- **Admin Config API**: 6 个 JWT+Admin 端点（GET 掩码返回、PUT 写回+自动备份、POST /test 连接测试、POST /backup、GET /backups、POST /restore）
- **前端**: `web-ui/src/components/SystemConfig.tsx` — Database/Redis/Server/Security/Backups 分区、连接测试按钮、保存/恢复；Admin 页新增 "Config" 标签
- **设计系统**: theme.css 新增 `--color-success` / `--color-success-hover` / `--color-success-subtle` tokens
- 无新增数据库迁移

### Verification
- `cargo clippy --workspace -D warnings` ✅
- `cargo test --workspace` ✅ (70 pass, 10 ignored)
- `npm run build` ✅

### 版本: 0.16.3 → **0.17.1**

## P4-J: Comprehensive Test Coverage (✅ 本次完成)

**目标**: 为全部 Rust 代码补充单元测试，行覆盖率 ≥90%（用户后放宽至 "覆盖率超过90就行了"）。

### 基础设施
- **Docker 测试环境**: `postgres:18-alpine` (:5432) + `redis:8-alpine` (:6379) + `minio/minio` S3 (:9000, bucket `pichost`) — 使此前 `#[ignore]` 的 DB/Redis/S3 集成测试可运行
- **Router 移入 lib**: `pichost-api/src/app.rs` 新增 `build_router`/`configure_app`/`init_storage_backends`（从 main.rs 移入），集成测试用 `tower::ServiceExt::oneshot` 驱动真实生产路由
- **测试 harness**: `pichost-api/tests/common/mod.rs` — `test_app()`、`create_user`/`create_admin`、`send_json`/`send_raw`、`tiny_png`、`multipart_image`、`create_invite`、`insert_user_direct`
- **dev-deps**: pichost-api 新增 `tower(util)`、`http-body-util`、`serial_test`、`argon2`、`rand`
- **S3 bugfix**: `storage/s3.rs get()` 错误映射改用结构化 `is_no_such_key()` 检测（原字符串匹配对 MinIO 返回的 "service error" 失效）

### 新增测试 (合计 555 个可运行，0 失败)
| 位置 | 测试数 | 覆盖内容 |
|------|--------|---------|
| `pichost-api/tests/` | auth 12, images 43, users+categories, admin, cache, oauth, gaps_*/gaps2_* | 全部路由 handler 成功+错误路径（真实 PG+Redis） |
| `pichost-api/src/` 各模块 | ~85 | 纯逻辑函数（upload helpers、middleware、config、metrics、html_escape） |
| `pichost-core/src/` | 92 个 in-crate + 3 个 S3 集成（MinIO） | config/crypto/error/models/storage 全覆盖 |
| `pichost-worker/src/` | 158 个 | processor/queue(Redis)/pipeline(真实 PG)/main/watermark/fonts |

### 覆盖率 (cargo llvm-cov, --include-ignored)
- **总行覆盖率: 91.56%**（10729 行，未覆盖 906 行）
- 100% 覆盖文件: app.rs, db, metrics, services/mod, middleware/metrics, models, error
- 唯一 0% 文件: `pichost-api/src/main.rs`（纯 bootstrap 入口，业务逻辑已在 app.rs 覆盖）

### 关键测试模式
- 环境变量隔离: `PichostEnvGuard`（capture/restore 全部 `PICHOST_*`，避免并行测试互扰）
- 测试使用 `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]`（current-thread runtime 会死锁 deadpool）
- 每个测试独立 PG pool（max 5 连接）避免连接耗尽；harness 共享连接
- 配置端点测试用 `#[serial]` + 清理 config.toml
- 需 PG/Redis/S3 的测试统一 `#[ignore]`-gated（CI 无 infra 时跳过），覆盖率用 `-- --include-ignored` 测量

### Verification
- `cargo test --workspace`（无 infra）✅ (313 pass, 0 fail；242 个 DB/Redis/S3 测试 ignored)
- `cargo test --workspace -- --include-ignored`（Docker infra）✅ (555 pass, 0 fail)
- `cargo clippy --workspace --all-targets -D warnings` ✅
- `cargo llvm-cov --workspace --ignore-filename-regex 'tests/|test_' -- --include-ignored` → **91.56%**

### 版本: 0.17.3

## P5-A: API 冒烟测试 CI 工作流 ✅ (本次完成)

**目标**: 完善 API 自动化测试，建立 PR 触发式冒烟测试 CI，确立新特性开发的冒烟测试设计模式。

### 测试覆盖审计结果
- **50 个 API 端点全部有集成测试覆盖**（`pichost-api/tests/` 198 个路由级测试），仅两个已知缺口（OAuth callback/link 成功路径，需 mock 外部 provider，优先级低）
- `health_test.rs` 原是死占位符（`assert!(true)`），已清理为说明性注释，真实 health 测试位于 `auth_test.rs`

### GitHub Actions 冒烟测试工作流
- **`.github/workflows/smoke-test.yml`**: PR 合入 `main`（open/sync/reopen）+ push 触发
- **服务容器**: PostgreSQL 18 + Redis 8 + MinIO（bucket `pichost` 通过 `minio/mc` 自动创建）
- **测试命令**: `cargo test --workspace -- --include-ignored`（~555 测试全量运行）
- **质量门**: `cargo clippy --workspace -- -D warnings`
- **缓存**: cargo registry/git/target 复用 e2e 缓存键模式
- 此前 `release.yml` 仅运行 313 个无基础设施测试，`e2e.yml` 仅 Playwright — 集成测试从未在 PR 上运行过

### 冒烟测试设计指南
- **`docs/superpowers/specs/2026-08-02-pichost-smoke-test-design.md`**: 建立新特性开发的冒烟测试设计模式
- 开发前: 列出新/改端点，设计成功+错误路径测试，确认测试文件归属
- 开发中: TDD 红-绿-重构，使用 `common/mod.rs` harness
- 开发后: `cargo test --workspace -- --include-ignored` + clippy
- 新特性 CRUD 端点覆盖要求: create/read/update/delete 的 success + not_found + 权限错误路径

### Verification
- `cargo clippy --workspace -D warnings` ✅
- `cargo test --workspace` ✅ (313 pass, 0 fail — 与基线一致)

### 版本: 0.17.4 → **0.17.5**

## i18n 国际化 ✅ (本次完成)

**目标**: 为 PicHost 增加中英双语支持 —— 后端 API 错误本地化 + 错误码信封，前端全量文案提取（~350 字符串），支持部署语言配置热加载。

### 后端 i18n 模块 (pichost-core)
- **`pichost-core/src/i18n.rs`**: `Language` 枚举 (En/ZhCN)，`I18n` 结构体 `t(locale, key)` / `t_args(locale, key, args)`，回退链 locale → en → key
- **全局单例**: `RwLock<Option<Arc<I18n>>>`，`I18n::global()` 带 5s mtime 懒检查热更新，`init_global`/`reload_global`/`maybe_reload`
- **消息目录**: `pichost-core/src/i18n/locales/{en,zh-CN}/messages.toml`（110 键），`include_str!` 内嵌；可选外部覆盖目录 `PICHOST_I18N_LOCALES_DIR`（按语言子目录 merge-override）
- **新配置**: `i18n.language`（env `PICHOST_I18N_LANGUAGE`，默认 "en"）+ `i18n.locales_dir`（env `PICHOST_I18N_LOCALES_DIR`，可选）

### 错误信封变更 (breaking)
- 所有 API 错误统一为 `{"error": <本地化消息>, "code": <错误键>}`
- 路由级键为点分式（`auth.invalid_credentials`、`image.not_found`、`validation.body_invalid`）；内部路径经 `AppError::code()` 输出下划线粗粒度码（`auth_failed`、`validation_error` 等），双约定为有意设计
- **Accept-Language 协商**: 请求头 → 部署 `i18n.language` → en；前端每个请求都携带 `Accept-Language: <UI语言>`（ky beforeRequest hook）
- **`JsonBody<T>` extractor**（`pichost-api/src/i18n_ext.rs`）替代 `Json<T>`：畸形 JSON / 错误 content-type → 422/415 + 本地化 `validation.body_invalid`，取代 axum 纯文本；另有 `Locale` extractor + `error_json`/`error_json_args`/`error_json_extra` 辅助

### 系统配置集成 (热加载)
- Config 服务读写 `[i18n] language` / `[i18n] locales_dir`；PUT `/admin/config` 与 POST `/admin/config/restore` 触发 `I18n::reload_global`，语言变更无需重启即生效
- Admin SystemConfig UI 新增 Localization 分区（语言选择器）

### 前端 (web-ui)
- **i18next 栈**: i18next ^26 + react-i18next ^17 + i18next-browser-languagedetector ^8
- **`src/i18n/`**: 初始化、`getCurrentLocale`、`applyLang`（同步 `<html lang>`），`{en,zh-CN}.json` 目录（364 键，键集相等性已测试），`types/i18next.d.ts` 类型化 t() 键，index.html FOUC 内联脚本，main.tsx 包 `I18nextProvider`
- **LanguageSwitcher**: NavBar + Login/Register 页面，localStorage 键 `pichost-locale`
- **格式模块**: `src/lib/format.ts` + `src/hooks/useFormat.ts` — 语言感知 formatBytes/formatDate/formatNumber（替代 5 处重复实现 + 硬编码 'en-US' 日期）
- **API 错误解析**: `src/api/errors.ts`（getErrorCode/isErrorCode），client.ts beforeError hook 从后端本地化 body 设置 error.message 并附加 code

### 验证
- `cargo test --workspace`（无 infra）✅ (324 pass, 0 fail)
- `cargo test --workspace -- --include-ignored`（Docker infra）✅ (575 pass, 0 fail；较 555 新增 20：i18n 模块单测、错误码断言、zh 协商、热加载、JsonBody 拒绝、config i18n 往返)
- 前端 vitest ✅ (42 pass)
- Playwright E2E ✅ (73 pass；新增 3 个 i18n spec：NavBar 切换持久化、登录页切换、SystemConfig 语言字段 → API 错误本地化 + 恢复)
- `cargo clippy --workspace -D warnings` ✅
- `npm run build` ✅

### 版本: 0.17.5 → **0.18.0**（Cargo.toml + web-ui/package.json 对齐 0.18.0）

## 响应式布局 ✅ (本次完成)

参考 `docs/superpowers/specs/2026-08-08-pichost-responsive-design.md`（方案 B：Tailwind 断点 + 少量共享组件）。

### 共享组件层（新增）
- **`useOverlay(onClose, enabled)` hook**: Escape 关闭 + body 滚动锁 + 覆盖层点击关闭，`enabled` 门控（修复 iOS Safari 关闭弹窗后页面滚动冻结的 CRITICAL）
- **`ui/Modal`**: 移动端（<sm）底部弹层 / ≥sm 居中面板，`role="dialog"`，保留 `.glass-modal` 类（E2E 定位符依赖），内容区 `min-h-0 flex-1` 可滚动
- **`ui/ConfirmDialog`**: 危险操作确认框，替换 3 处原生 confirm/alert
- **`ui/Sheet`**: 左侧滑出抽屉（分类筛选）
- **`clampLeft`**: 弹出层视口钳制（GlassSelect/DropdownMenu 窄屏防溢出）

### 页面改造
- **MobileNav 汉堡菜单**: <768px NavBar 收起链接，抽屉内含导航 + 用户操作（设置/管理/登出）+ 主题/语言切换
- **Gallery**: 移动端分类按钮 → Sheet 抽屉复用 CategoryTree；网格 `grid-cols-2 sm:3 md:3 lg:4 xl:5`；选择按钮触屏常显
- **CategoryTree**: 每行 ⋯ 按钮（触屏菜单，右键保留），菜单锚点双触发 + 视口钳制；`w-80` 弹窗 → Modal/ConfirmDialog
- **弹窗迁移**: EditUserDialog/CreateInviteDialog/StorageConfigSection/SystemConfig/Gallery 全部改用共享 Modal/ConfirmDialog（含 backdrop-filter 包含块 hoist 处理）
- **Admin 表格卡片化**: <sm 卡片列表（`data-testid="user-card"/"invite-card"`），≥sm 表格 + `overflow-x-auto`
- **全局**: `overflow-x: clip` 守卫（修复 /gallery 66px 溢出）、容器 padding 断点化、WatermarkSettings/PreprocessingSettings 网格断点、DropZone 触屏点击区、Admin 标签栏滚动兜底、ImageDetail 重命名铅笔触屏常显

### 验证
- `cargo test --workspace` ✅ (324 pass, 0 fail — 后端零改动)
- `cargo clippy --workspace -D warnings` ✅
- 前端 vitest ✅ (60 pass — 新增 useOverlay 5 测 + Modal/ConfirmDialog 5 测 + clampLeft 3 测 + i18n 扩展)
- Playwright E2E ✅ (94 pass, 1 skip — 新增 mobile-nav 3、mobile-gallery 1、mobile-admin 3、responsive 6、settings/admin/categories/gallery/upload/image-detail 扩展)
- `npm run build` ✅
- 全分支评审: 1 CRITICAL（useOverlay 滚动锁门控）+ 4 MEDIUM + 1 LOW 全部修复（5 commits），scoped re-review ALL ADDRESSED

### 版本: 0.18.0 → **0.19.0**（Cargo.toml + web-ui/package.json 对齐 0.19.0，CHANGELOG 更新）

## 待实施

| 阶段 | 主题 | 依赖 |
|------|------|------|
| i18n 扩展 | 新增语言 (ja/ko/...) — 后端 `locales/{lang}/messages.toml` + `Language` 枚举扩展，前端 `src/i18n/locales/{lang}.json` + LanguageSwitcher 枚举，键集相等性测试 | i18n 已落地 |
| 触屏增强 | CategoryTree 长按手势（当前 ⋯ 按钮已覆盖触屏，长按列为后续增强） | 响应式已落地 |

当前计划内阶段（P0–P4-I）+ i18n + 响应式布局已全部完成。下一步待定（可根据用户新需求或 README/AGENTS 中记录的已知限制制定新计划）。
