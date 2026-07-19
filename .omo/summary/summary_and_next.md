# PicHost 项目进度

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

## 待实施

| 阶段 | 主题 | 依赖 |
|------|------|------|
| P4-C | 图库分类/目录 | P4-A ✅ → **✅ 完成 (0.16.0)** |
| P4-D | 服务端水印 | 无 |
| P4-E | 客户端图片预处理 | 无 |
| P4-F | 文件名保留 + 重命名 | 无 |
| P4-G | 设置入口优化 | 无 |
| P4-H | 软件打包 + 自动化发布 | 无 |
| P4-I | 系统配置管理 | 无 |

**B–I 互不依赖**，可在 P4-A 完成后并行开发。
**下一步**: P4-D 或 P4-E 任选其一。
