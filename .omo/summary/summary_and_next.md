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
