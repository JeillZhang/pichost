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

## 剩余待开发特性

- **P1**: 无
- **P2**: OAuth 登录, 邀请码注册, 清理临时文件

## 建议下一步开发
考虑 P2 特性: OAuth 登录 或 邀请码注册机制。
