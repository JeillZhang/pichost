# PicHost UI 视觉重设计 — 设计文档

> **日期**: 2026-08-02  
> **目标**: light 主题配色和谐度 + Apple 风格毛玻璃效果增强  
> **方向**: 冷调 slate-indigo 专业沉稳风 + 色调染色分层玻璃  

---

## 1. 问题诊断

### 1.1 当前配色冲突

light 主题存在三色调冲突：

| 元素 | 当前值 | 色温 |
|------|--------|------|
| 背景 | `#f5f6f8` cool gray | 冷 |
| 文字 | `#1c1917` stone-900 | 暖 |
| 强调色 | `#0891b2` teal | 中 |

暖调 stone 文字 + 冷调 gray 背景 + teal 强调色 = **三种色温在同一画面上冲突**，是"配色不搭"的根因。

### 1.2 当前玻璃缺陷

- **无色调染色**：`--glass-bg: rgba(255,255,255,0.72)` 是纯白半透明，Apple 风格玻璃带有微妙的底色色调
- **无分层景深**：所有层级共享同一透明度（72%）和模糊值（20px）
- **饱和度偏高**：`saturate(180%)` 在 light 模式下泛白
- **高光线缺失**：部分玻璃面有 `inset 0 1px 0` 高光，部分没有，缺少一致性的镜面反射感

### 1.3 硬编码颜色泄漏

| 文件 | 问题 |
|------|------|
| `Login.tsx` / `Register.tsx` | 紫色渐变 Logo（`indigo-400 → purple-400`），与 NavBar 的 teal 不一致 |
| `PreprocessingSettings.tsx` | `text-blue-400`、`border-blue-500` 等 Tailwind 默认色 |
| `PreprocessingStatus.tsx` | `text-blue-400` 硬编码 |
| `AdminStats.tsx` | `#a78bfa` violet 硬编码 |
| `ProtectedRoute.tsx` | `bg-gray-950` 直接使用 Tailwind 灰度 |

---

## 2. 配色体系

### 2.1 Light 主题 Token

策略：全部统一到冷调 slate 体系 + indigo 强调色。

#### 背景层

```css
--color-bg: #f8fafc;          /* slate-50, 替换 #f5f6f8 */
--color-bg-subtle: #f1f5f9;   /* slate-100, 替换 #eeeef2 */
--color-surface: rgba(15, 23, 42, 0.03);  /* slate 基底替代纯黑 */
```

#### 文字层

```css
--color-text-primary: #0f172a;    /* slate-900, 替换 #1c1917 */
--color-text-secondary: #475569;  /* slate-600, 替换 #57534e */
--color-text-muted: #94a3b8;      /* slate-400, 替换 #a8a29e */
--color-text-on-accent: #ffffff;
```

#### 强调色（indigo 替代 teal）

```css
--color-accent: #4f46e5;                          /* indigo-600 */
--color-accent-hover: #4338ca;                    /* indigo-700 */
--color-accent-active: #3730a3;                   /* indigo-800 */
--color-accent-subtle: rgba(79, 70, 229, 0.07);
--color-accent-glow: rgba(79, 70, 229, 0.12);
--color-accent-strong: rgba(79, 70, 229, 0.20);

/* 渐变 */
--color-accent-gradient: linear-gradient(135deg, #4f46e5 0%, #6366f1 50%, #4f46e5 100%);
--color-accent-gradient-hover: linear-gradient(135deg, #4338ca 0%, #818cf8 50%, #4338ca 100%);
```

#### 语义色

```css
--color-danger: #ef4444;
--color-danger-hover: #dc2626;
--color-danger-subtle: rgba(239, 68, 68, 0.07);
--color-danger-border: rgba(239, 68, 68, 0.15);

--color-success: #16a34a;
--color-success-hover: #15803d;
--color-success-subtle: rgba(22, 163, 74, 0.07);

--color-warning: #f59e0b;
--color-warning-hover: #d97706;
--color-warning-subtle: rgba(245, 158, 11, 0.07);
```

#### 边框

```css
--color-border: rgba(15, 23, 42, 0.06);
--color-border-hover: rgba(15, 23, 42, 0.12);
--color-border-strong: rgba(15, 23, 42, 0.08);
```

#### 阴影

```css
--shadow-sm: 0 1px 2px rgba(15, 23, 42, 0.04);
--shadow-md: 0 1px 3px rgba(15, 23, 42, 0.04), 0 4px 16px rgba(15, 23, 42, 0.06);
--shadow-lg: 0 1px 3px rgba(15, 23, 42, 0.04), 0 12px 40px rgba(15, 23, 42, 0.10);
--shadow-glow: 0 0 32px var(--color-accent-glow);
```

### 2.2 Dark 主题 Token

暗色主题同步调整为 slate-indigo 体系，invert 明暗关系。

```css
.dark {
  --color-bg: #020617;                              /* slate-950 */
  --color-bg-subtle: #0f172a;                       /* slate-900 */
  --color-surface: rgba(255, 255, 255, 0.03);
  --color-surface-hover: rgba(255, 255, 255, 0.06);
  --color-surface-elevated: rgba(255, 255, 255, 0.05);

  --color-text-primary: #f1f5f9;                    /* slate-100 */
  --color-text-secondary: #94a3b8;                  /* slate-400 */
  --color-text-muted: #64748b;                      /* slate-500 */
  --color-text-on-accent: #ffffff;

  --color-accent: #818cf8;                          /* indigo-400, 暗色模式下提亮 */
  --color-accent-hover: #6366f1;                    /* indigo-500 */
  --color-accent-active: #4f46e5;
  --color-accent-subtle: rgba(129, 140, 248, 0.10);
  --color-accent-glow: rgba(129, 140, 248, 0.16);
  --color-accent-strong: rgba(129, 140, 248, 0.25);

  --color-accent-gradient: linear-gradient(135deg, #6366f1 0%, #818cf8 50%, #6366f1 100%);
  --color-accent-gradient-hover: linear-gradient(135deg, #4f46e5 0%, #a5b4fc 50%, #4f46e5 100%);

  --color-danger: #fca5a5;
  --color-danger-hover: #f87171;
  --color-danger-subtle: rgba(252, 165, 165, 0.08);
  --color-danger-border: rgba(252, 165, 165, 0.15);

  --color-success: #86efac;
  --color-success-hover: #4ade80;
  --color-success-subtle: rgba(134, 239, 172, 0.08);

  --color-warning: #fde68a;
  --color-warning-hover: #fbbf24;
  --color-warning-subtle: rgba(253, 230, 138, 0.08);

  --color-border: rgba(255, 255, 255, 0.06);
  --color-border-hover: rgba(255, 255, 255, 0.12);
  --color-border-strong: rgba(255, 255, 255, 0.08);

  --shadow-sm: 0 1px 2px rgba(0, 0, 0, 0.20);
  --shadow-md: 0 4px 16px rgba(0, 0, 0, 0.30);
  --shadow-lg: 0 8px 40px rgba(0, 0, 0, 0.45);
  --shadow-glow: 0 0 24px var(--color-accent-glow);
}
```

---

## 3. 毛玻璃分层体系

### 3.1 设计原则

Apple 玻璃三个关键特征在此系统中实现：

1. **色调染色** — `--glass-tint-base: oklch(0.97 0.005 260)` 冷调白基底色，通过 `color-mix()` 按比例混入各层
2. **分层景深** — 4 层透明度/模糊度梯度，模拟"离背景越远越看不清背后"
3. **镜面高光** — `inset 0 1px 0 var(--glass-highlight)` 统一加到所有玻璃类

### 3.2 Token 定义

```css
/* ── 共享玻璃基础 ── */
--glass-highlight: rgba(255, 255, 255, 0.55);     /* 顶部镜面高光线 */
--glass-tint-base: oklch(0.97 0.005 260);          /* 冷调白基底色 */
--glass-border-base: rgba(15, 23, 42, 0.05);       /* 基础边框 */
--glass-border-strong: rgba(15, 23, 42, 0.08);     /* 强调边框 */

/* ── Layer 1: 卡片 — 图片卡片、设置面板、Dashboard 行 ── */
--glass-layer-card-opacity: 0.65;
--glass-layer-card-blur: 16px;
--glass-layer-card-saturate: 140%;

/* ── Layer 2: 悬浮面板 — dropdown、详情卡片、sidebar ── */
--glass-layer-elevated-opacity: 0.78;
--glass-layer-elevated-blur: 24px;
--glass-layer-elevated-saturate: 150%;

/* ── Layer 3: NavBar — 固定顶部栏 ── */
--glass-layer-nav-opacity: 0.88;
--glass-layer-nav-blur: 32px;
--glass-layer-nav-saturate: 160%;

/* ── Layer 4: Modal — 弹窗/对话框 ── */
--glass-layer-modal-opacity: 0.90;
--glass-layer-modal-blur: 40px;
--glass-layer-modal-saturate: 150%;
```

### 3.3 对比：当前 vs 新

| 层级 | 当前透明度 | 新透明度 | 当前模糊 | 新模糊 | Sat (当前→新) | 色调染色 | 高光线 |
|------|----------|----------|---------|--------|--------------|---------|--------|
| 卡片 | 72% | ~65% | 20px | 16px | 180%→140% | ❌→✅ | ❌→✅ |
| 悬浮 | 72% | ~78% | 20px | 24px | 180%→150% | ❌→✅ | ❌→✅ |
| NavBar | 72% | ~88% | 20px | 32px | 270%→160% | ❌→✅ | ❌→✅ |
| Modal | 72% | ~90% | 25px | 40px | 180%→150% | ❌→✅ | ❌→✅ |

### 3.4 色调染色实现

每层玻璃背景通过 `color-mix()` 混合冷调白基底色，替代纯 `rgba(255,255,255,x)`：

```css
/* 卡片层 */
.glass {
  background: color-mix(in oklch, var(--glass-tint-base) calc(var(--glass-layer-card-opacity) * 100%), transparent);
}

/* NavBar 层 */
.glass-nav {
  background: color-mix(in oklch, var(--glass-tint-base) calc(var(--glass-layer-nav-opacity) * 100%), transparent);
}
```

冷调白（oklch 260° hue）带极微量蓝紫调，让玻璃看起来像"有厚度的磨砂冰"，而不是"透明塑料"。

---

## 4. 组件级修复

### 4.1 Login.tsx / Register.tsx

**问题**：Logo 使用独立紫色渐变（`from-indigo-400 to-purple-400`），与 NavBar 品牌渐变不一致。

**修复**：
```tsx
// Before
className="bg-gradient-to-r from-indigo-400 to-purple-400 bg-clip-text text-transparent"

// After
className="bg-gradient-to-r from-[var(--color-accent)] via-[var(--color-accent-hover)] to-[var(--color-accent)] bg-clip-text text-transparent"
```

### 4.2 PreprocessingSettings.tsx

**问题**：使用 Tailwind 默认色彩类名（`text-blue-400`、`border-blue-500`、`text-red-400`）。

**修复**：全部替换为：
- `text-blue-400` → `text-[var(--color-accent)]`
- `border-blue-500/20` → `border-[var(--color-accent-subtle)]`
- `text-red-400` → `text-[var(--color-danger)]`

### 4.3 PreprocessingStatus.tsx

**问题**：硬编码 `text-blue-400` 和 `border-blue-500/20`。

**修复**：同 PreprocessingSettings，替换为 `var(--color-accent)` 系 token。

### 4.4 AdminStats.tsx

**问题**：icon 颜色 `#a78bfa`（violet）与 indigo 强调色不匹配。

**修复**：
```tsx
// Before
style={{ color: '#a78bfa' }}

// After
style={{ color: 'var(--color-accent)' }}
```

### 4.5 ProtectedRoute.tsx

**问题**：loading spinner 使用 `bg-gray-950`，与 token 体系脱离。

**修复**：
```tsx
// Before
className="min-h-screen bg-gray-950 ..."

// After
className="min-h-screen bg-[var(--color-bg)] ..."
```

---

## 5. 不动的部分

- **所有组件的结构、布局、功能逻辑**不变
- **Tailwind CSS 4 配置方式**不变（CSS-first，`@import "tailwindcss"`）
- **主题切换机制**不变（Zustand store + `.dark` class）
- **玻璃 CSS 类名**不变（`.glass`、`.glass-elevated`、`.glass-nav`、`.glass-modal` 保留）——内部实现改 token，外部接口不变
- **Radius、Transition token** 不变
- **字体系统**不变（Inter + Outfit）

---

## 6. 验证计划

1. **`npm run build`** — 确保 TypeScript + Vite 构建通过
2. **视觉检查** — light/dark 模式下的所有页面：
   - Dashboard（DropZone、UploadCard、配额进度条、最近图片）
   - Gallery（侧栏 CategoryTree、图片网格、选择工具栏、删除对话框）
   - ImageDetail（预览卡片、信息面板、链接卡片）
   - Settings（分区导航、水印/存储/预处理配置面板）
   - Admin（统计卡片、用户表格、邀请码、系统配置）
   - Login / Register（表单卡片）
   - NavBar（用户下拉菜单）
3. **玻璃分层验证** — 检查不同层级玻璃的透明度/模糊度梯度是否明显
4. **色调染色** — 确认玻璃带冷调白底而非纯白
5. **高光线** — 确认所有玻璃层顶部有 `inset 0 1px 0` 镜面反射感

---

## 7. 文件变更清单

| 文件 | 改动性质 | 优先级 |
|------|---------|--------|
| `web-ui/src/theme.css` | token 全面重写（配色 + 玻璃分层） | P0 |
| `web-ui/src/index.css` | `.glass-*` 类适配新 token 结构 | P0 |
| `web-ui/src/pages/Login.tsx` | 品牌渐变统一 | P1 |
| `web-ui/src/pages/Register.tsx` | 品牌渐变统一 | P1 |
| `web-ui/src/components/PreprocessingSettings.tsx` | 硬编码色 → token | P1 |
| `web-ui/src/components/PreprocessingStatus.tsx` | 硬编码色 → token | P1 |
| `web-ui/src/pages/admin/AdminStats.tsx` | 硬编码色 → token | P1 |
| `web-ui/src/components/ProtectedRoute.tsx` | loading spinner 颜色 token 化 | P2 |
