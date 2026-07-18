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

## 剩余待开发特性

- **P2 (remaining)**: OAuth 登录, 图片库增强(分页/搜索), 多文件并发拖拽上传,
  用户存储配额, 批量管理, /metrics Prometheus 端点, CDN 集成, 水平扩展

## 建议下一步开发
OAuth 登录 (GitHub/Google) 或 图片库增强 (分页/无限滚动/搜索)。
