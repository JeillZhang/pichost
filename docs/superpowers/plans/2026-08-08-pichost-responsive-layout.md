# PicHost Web 自适应布局 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 全面移动端适配——手机/平板/桌面多设备正常浏览与操作（纯前端，无后端/DB 变更）。

**Architecture:** 方案 B（设计规格 §2）：Tailwind 断点工具类全量适配 + 新增 4 个轻量组件（MobileNav / Sheet / Modal / ConfirmDialog）+ 1 个 hook（useOverlay）+ 2 处改造（CategoryTree ⋯ 菜单、Admin 表格卡片化）。组件自包含、无新依赖，沿用 `.glass-*` 设计 token。

**Tech Stack:** React 19 / Vite 8 / Tailwind CSS 4（默认断点 sm=640 md=768 lg=1024 xl=1280）/ TypeScript 7 / vitest（jsdom）/ Playwright E2E。

## Global Constraints

- 前端验证门：`npm run build`（tsc -b && vite build）+ `npx vitest run` + `npx playwright test`（需 Docker PG+Redis，见 e2e 配置）
- 后端无改动；`cargo clippy --workspace -- -D warnings` 只需确认无回归（可跳过）
- 版本：`0.18.0` → `0.19.0`（Cargo.toml + web-ui/package.json 对齐，CHANGELOG 追加条目）
- 不引入新 npm 依赖（禁止 Radix 等）
- **回归红线**：`admin.spec.ts:76` 用 `page.locator('.glass-modal')` 定位弹窗——所有新弹窗面板**必须保留 `glass-modal` 类**
- 移动端 E2E 用 `test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })` 按文件覆盖
- Playwright 串行执行（workers:1，共享后端实例）；E2E 前需 `node web-ui/e2e/reset-test-env.mjs`（config 已内置）
- i18n：新 key 必须 en/zh-CN 双语同步，`i18n.test.ts` 键集相等性测试强制保证
- 桌面端（≥1024px）视觉零回归——现有 73 个 E2E specs 必须全部保持通过
- Rust 规则（≤50 行函数、≤120 字符行）仅约束 Rust 代码，本计划无 Rust 改动

---

## Agent Worker Instructions

- **Required sub-skills:** superpowers:subagent-driven-development（每任务独立子代理 + 双阶段评审）
- **Execution mode:** subagent-driven-development (preferred)
- **Required verification:** `npm run build` + `npx vitest run`；涉及 UI 行为的任务加 `npx playwright test e2e/specs/<spec>`（需 Docker PG+Redis 运行中）
- **Version bump reminder:** 全部任务完成后执行 T16 版本 bump（0.18.0 → 0.19.0）
- **任务间依赖纪律：** `depends_on` 严格按序；被依赖任务的产出接口（props/函数签名）在"Interfaces"块声明，不得自行改名

---

### Task 0: Add useOverlay hook

- **depends_on:** [] | **breaking:** false

**Files:**
- Create: `web-ui/src/hooks/useOverlay.ts`
- Test: `web-ui/src/hooks/useOverlay.test.ts`

**Interfaces:**
- Produces: `useOverlay(onClose: () => void): { overlayProps: { onMouseDown: (e: React.MouseEvent) => void } }` — Escape 关闭 + body 滚动锁 + 覆盖层点击关闭

- [ ] **Step 1: Write the failing test**

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import useOverlay from './useOverlay'

// Test harness: a tiny component consuming the hook
function Harness({ onClose }: { onClose: () => void }) {
  const { overlayProps } = useOverlay(onClose)
  return <div data-testid="overlay" {...overlayProps} />
}

function renderHarness(onClose: () => void): Root {
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  act(() => root.render(<Harness onClose={onClose} />))
  return root
}

describe('useOverlay', () => {
  beforeEach(() => {
    document.body.style.overflow = ''
  })

  it('locks body scroll while mounted and restores on unmount', () => {
    const onClose = vi.fn()
    const root = renderHarness(onClose)
    expect(document.body.style.overflow).toBe('hidden')
    act(() => root.unmount())
    expect(document.body.style.overflow).toBe('')
  })

  it('calls onClose on Escape keydown', () => {
    const onClose = vi.fn()
    const root = renderHarness(onClose)
    act(() => {
      document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' }))
    })
    expect(onClose).toHaveBeenCalledTimes(1)
    act(() => root.unmount())
  })

  it('calls onClose when overlay itself is clicked', () => {
    const onClose = vi.fn()
    const root = renderHarness(onClose)
    act(() => {
      document.querySelector('[data-testid="overlay"]')!.dispatchEvent(
        new MouseEvent('mousedown', { bubbles: true }),
      )
    })
    expect(onClose).toHaveBeenCalledTimes(1)
    act(() => root.unmount())
  })

  it('does NOT close when a click inside the panel bubbles to the overlay (stopPropagation respected)', () => {
    const onClose = vi.fn()
    const container = document.createElement('div')
    document.body.appendChild(container)
    const root = createRoot(container)
    act(() =>
      root.render(
        <div data-testid="panel" onMouseDown={(e) => e.stopPropagation()}>
          <Harness onClose={onClose} />
        </div>,
      ),
    )
    act(() => {
      document.querySelector('[data-testid="panel"]')!.dispatchEvent(
        new MouseEvent('mousedown', { bubbles: true }),
      )
    })
    expect(onClose).not.toHaveBeenCalled()
    act(() => root.unmount())
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx vitest run src/hooks/useOverlay.test.ts`
Expected: FAIL — "Failed to resolve import './useOverlay'"

- [ ] **Step 3: Write minimal implementation**

```ts
import { useEffect } from 'react'

/**
 * Shared overlay behavior for Modal / Sheet / MobileNav:
 * Escape closes, body scroll locks while open, overlay click closes.
 * Panel clicks must stopPropagation to avoid closing.
 */
export default function useOverlay(onClose: () => void) {
  useEffect(() => {
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose()
    }
    document.addEventListener('keydown', handleKeyDown)
    const prevOverflow = document.body.style.overflow
    document.body.style.overflow = 'hidden'
    return () => {
      document.removeEventListener('keydown', handleKeyDown)
      document.body.style.overflow = prevOverflow
    }
  }, [onClose])

  return {
    overlayProps: {
      onMouseDown: (e: React.MouseEvent) => {
        // Only close when the click target is the overlay itself,
        // not bubbled from the panel (panel must stopPropagation).
        if (e.target === e.currentTarget) onClose()
      },
    },
  }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web-ui && npx vitest run src/hooks/useOverlay.test.ts && npx vitest run`
Expected: PASS (useOverlay 4 tests + 全量单测无回归)

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/hooks/useOverlay.ts web-ui/src/hooks/useOverlay.test.ts
git commit -m "feat: add useOverlay hook for modal/sheet/mobile-nav"
```

---

### Task 1: Add responsive i18n keys

- **depends_on:** [] | **breaking:** false

**Files:**
- Modify: `web-ui/src/i18n/locales/en.json`
- Modify: `web-ui/src/i18n/locales/zh-CN.json`
- Test: `web-ui/src/i18n/i18n.test.ts`

**Interfaces:**
- Produces keys (all tasks consume): `nav.menu`、`gallery.allCategories`、`categoryTree.moreActions`、`modal.close`、`confirmDialog.confirm`、`adminUsers.deleteTitle`

- [ ] **Step 1: Write the failing test** — append to `i18n.test.ts`

```ts
it('responsive feature keys exist in both locales', async () => {
  const en = (await import('./locales/en.json')).default as Record<string, unknown>
  const zh = (await import('./locales/zh-CN.json')).default as Record<string, unknown>
  expect(en.nav.menu).toBeTruthy()
  expect(zh.nav.menu).toBeTruthy()
  expect(en.gallery.allCategories).toBeTruthy()
  expect(zh.gallery.allCategories).toBeTruthy()
  expect(en.categoryTree.moreActions).toBeTruthy()
  expect(zh.categoryTree.moreActions).toBeTruthy()
  expect(en.modal.close).toBeTruthy()
  expect(zh.modal.close).toBeTruthy()
  expect(en.confirmDialog.confirm).toBeTruthy()
  expect(zh.confirmDialog.confirm).toBeTruthy()
  expect(en.adminUsers.deleteTitle).toBeTruthy()
  expect(zh.adminUsers.deleteTitle).toBeTruthy()
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx vitest run src/i18n/i18n.test.ts`
Expected: FAIL — key-set equality assertion fails (keys added only to one side) or `nav.menu` undefined

- [ ] **Step 3: Implement keys** — add to BOTH locale files（插入到对应对象内，勿覆盖已有字段）:

```json
// en.json 新增字段
"nav": { ..., "menu": "Menu" },
"gallery": { ..., "allCategories": "All Categories" },
"categoryTree": { ..., "moreActions": "More actions" },
"modal": { "close": "Close" },
"confirmDialog": { "confirm": "Confirm" },
"adminUsers": { ..., "deleteTitle": "Delete User" }
```

```json
// zh-CN.json 新增字段
"nav": { ..., "menu": "菜单" },
"gallery": { ..., "allCategories": "全部分类" },
"categoryTree": { ..., "moreActions": "更多操作" },
"modal": { "close": "关闭" },
"confirmDialog": { "confirm": "确认" },
"adminUsers": { ..., "deleteTitle": "删除用户" }
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web-ui && npx vitest run src/i18n/i18n.test.ts`
Expected: PASS (全部 i18n 测试，含既有键集相等性回归)

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/i18n/locales/en.json web-ui/src/i18n/locales/zh-CN.json web-ui/src/i18n/i18n.test.ts
git commit -m "feat: add responsive i18n keys (nav.menu, modal, confirmDialog)"
```

---

### Task 2: Add Modal + ConfirmDialog components

- **depends_on:** [T0] | **breaking:** false

**Files:**
- Create: `web-ui/src/components/ui/Modal.tsx`
- Create: `web-ui/src/components/ui/ConfirmDialog.tsx`
- Test: `web-ui/src/components/ui/Modal.test.tsx`

**Interfaces:**
- Consumes: `useOverlay` from Task 0
- Produces:
  - `Modal({ open: boolean; onClose: () => void; title?: string; children: ReactNode; footer?: ReactNode; size?: 'sm' | 'md' })`
  - `ConfirmDialog({ open: boolean; onClose: () => void; onConfirm: () => void; title: string; message: string; confirmLabel: string; cancelLabel?: string; danger?: boolean; pending?: boolean })`

**关键约束：** 面板保留 `glass-modal` 类（`admin.spec.ts` 依赖此定位符）；移动端（<sm）底部弹层：`items-end` + 全宽 + `rounded-t-2xl rounded-b-none`；≥sm 居中：`items-center justify-center p-4`。

- [ ] **Step 1: Write the failing test**

```tsx
import { describe, it, expect, vi } from 'vitest'
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import Modal from './Modal'
import ConfirmDialog from './ConfirmDialog'

function render(node: React.ReactNode): Root {
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  act(() => root.render(node))
  return root
}

describe('Modal', () => {
  it('renders nothing when closed', () => {
    const root = render(<Modal open={false} onClose={vi.fn()}>x</Modal>)
    expect(document.querySelector('.glass-modal')).toBeNull()
    act(() => root.unmount())
  })

  it('renders panel with glass-modal class and closes on Escape', () => {
    const onClose = vi.fn()
    const root = render(<Modal open onClose={onClose}>body</Modal>)
    const panel = document.querySelector('.glass-modal')
    expect(panel).toBeTruthy()
    expect(document.body.style.overflow).toBe('hidden')
    act(() => document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' })))
    expect(onClose).toHaveBeenCalledTimes(1)
    act(() => root.unmount())
  })

  it('closes when overlay (not panel) is clicked', () => {
    const onClose = vi.fn()
    const root = render(<Modal open onClose={onClose}>body</Modal>)
    const overlay = document.querySelector('[data-testid="modal-overlay"]')!
    act(() => overlay.dispatchEvent(new MouseEvent('mousedown', { bubbles: true })))
    expect(onClose).toHaveBeenCalledTimes(1)
    act(() => root.unmount())
  })

  it('renders title', () => {
    const root = render(<Modal open onClose={vi.fn()} title="Hello">body</Modal>)
    expect(document.querySelector('.glass-modal')!.textContent).toContain('Hello')
    act(() => root.unmount())
  })
})

describe('ConfirmDialog', () => {
  it('renders message and confirm button; confirm triggers onConfirm', () => {
    const onConfirm = vi.fn()
    const onClose = vi.fn()
    const root = render(
      <ConfirmDialog
        open
        onClose={onClose}
        onConfirm={onConfirm}
        title="Delete?"
        message="Are you sure?"
        confirmLabel="Delete"
      />,
    )
    expect(document.querySelector('.glass-modal')!.textContent).toContain('Are you sure?')
    const confirmBtn = [...document.querySelectorAll('button')].find((b) => b.textContent === 'Delete')!
    act(() => confirmBtn.click())
    expect(onConfirm).toHaveBeenCalledTimes(1)
    act(() => root.unmount())
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx vitest run src/components/ui/Modal.test.tsx`
Expected: FAIL — cannot resolve `./Modal`

- [ ] **Step 3: Write minimal implementation**

```tsx
// ui/Modal.tsx
import { type ReactNode } from 'react'
import { X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import useOverlay from '../../hooks/useOverlay'

interface ModalProps {
  open: boolean
  onClose: () => void
  title?: string
  children: ReactNode
  footer?: ReactNode
  size?: 'sm' | 'md'
}

export default function Modal({
  open,
  onClose,
  title,
  children,
  footer,
  size = 'md',
}: ModalProps) {
  const { t } = useTranslation()
  const { overlayProps } = useOverlay(onClose)
  if (!open) return null

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center sm:items-center sm:p-4">
      <div
        data-testid="modal-overlay"
        {...overlayProps}
        className="absolute inset-0 bg-black/50 backdrop-blur-sm"
      />
      <div
        className={`glass-modal relative flex max-h-[90dvh] w-full flex-col overflow-hidden rounded-t-2xl sm:rounded-xl ${
          size === 'sm' ? 'sm:max-w-sm' : 'sm:max-w-md'
        }`}
      >
        {(title || footer) && (
          <div className="flex items-center justify-between px-5 pt-4">
            <h2
              className="text-lg font-semibold"
              style={{ color: 'var(--color-text-primary)', fontFamily: "'Outfit', system-ui, sans-serif" }}
            >
              {title}
            </h2>
            <button
              onClick={onClose}
              aria-label={t('modal.close')}
              className="rounded-lg p-1 transition-colors hover:bg-[var(--color-surface-hover)]"
              style={{ color: 'var(--color-text-muted)' }}
            >
              <X className="h-5 w-5" />
            </button>
          </div>
        )}
        <div className="overflow-y-auto px-5 py-4">{children}</div>
        {footer && (
          <div className="flex justify-end gap-3 border-t px-5 py-3" style={{ borderColor: 'var(--color-border)' }}>
            {footer}
          </div>
        )}
      </div>
    </div>
  )
}
```

```tsx
// ui/ConfirmDialog.tsx
import { useTranslation } from 'react-i18next'
import Modal from './Modal'

interface ConfirmDialogProps {
  open: boolean
  onClose: () => void
  onConfirm: () => void
  title: string
  message: string
  confirmLabel: string
  cancelLabel?: string
  danger?: boolean
  pending?: boolean
}

export default function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  message,
  confirmLabel,
  cancelLabel,
  danger = false,
  pending = false,
}: ConfirmDialogProps) {
  const { t } = useTranslation()
  return (
    <Modal open={open} onClose={onClose} title={title} size="sm">
      <p className="text-sm leading-relaxed" style={{ color: 'var(--color-text-secondary)' }}>
        {message}
      </p>
      <div className="mt-5 flex justify-end gap-3">
        <button onClick={onClose} disabled={pending} className="btn-ghost">
          {cancelLabel ?? t('common.cancel')}
        </button>
        <button
          onClick={onConfirm}
          disabled={pending}
          className="btn-accent"
          style={danger ? { background: 'var(--color-danger)', color: 'white' } : undefined}
        >
          {confirmLabel}
        </button>
      </div>
    </Modal>
  )
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web-ui && npx vitest run src/components/ui/Modal.test.tsx && npx vitest run`
Expected: PASS (6 tests + 全量无回归)

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/components/ui/Modal.tsx web-ui/src/components/ui/ConfirmDialog.tsx web-ui/src/components/ui/Modal.test.tsx
git commit -m "feat: add shared Modal and ConfirmDialog components"
```

---

### Task 3: Add MobileNav hamburger menu + NavBar integration

- **depends_on:** [T0, T1] | **breaking:** false

**Files:**
- Create: `web-ui/src/components/MobileNav.tsx`
- Modify: `web-ui/src/components/NavBar.tsx`
- Test: `web-ui/e2e/specs/mobile-nav.spec.ts`

**Interfaces:**
- Consumes: `nav.menu` key (Task 1), `useOverlay` (Task 0), `useAuthStore`, `useNavigate`, `NavLink` — all existing
- Produces: `MobileNav({ open: boolean; onClose: () => void })` rendered inside NavBar

**回归红线：** 现有 `i18n.spec.ts`、`00-auth.spec.ts` 中 NavBar 相关断言（切换语言、登出入口）必须保持通过——<md 链接隐藏不能破坏 ≥md 渲染；登出逻辑仍走 `logout() + navigate('/login')`。

- [ ] **Step 1: Write the failing E2E test**

```ts
// e2e/specs/mobile-nav.spec.ts
import { test, expect } from '@playwright/test'
import { seedUserSession } from '../helpers/auth'

test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

test.describe.serial('mobile nav', () => {
  test('hamburger opens drawer with nav links and user actions', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/dashboard')

    // Desktop links are hidden on mobile
    await expect(page.getByRole('link', { name: 'Dashboard' })).toHaveCount(0)

    // Open hamburger menu
    const menuButton = page.getByRole('button', { name: /menu|菜单/i })
    await menuButton.click()
    await expect(page.getByRole('link', { name: /dashboard|仪表盘/i }).first()).toBeVisible()

    // Navigate via drawer
    await page.getByRole('link', { name: /gallery|图库/i }).first().click()
    await expect(page).toHaveURL(/\/gallery/)
  })

  test('drawer closes on Escape and overlay click', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/dashboard')
    await page.getByRole('button', { name: /menu|菜单/i }).click()
    await page.keyboard.press('Escape')
    await expect(page.getByRole('link', { name: /dashboard|仪表盘/i }).first()).toBeHidden()
  })

  test('logout reachable from mobile drawer', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/dashboard')
    await page.getByRole('button', { name: /menu|菜单/i }).click()
    await page.getByRole('button', { name: /logout|退出登录/i }).click()
    await expect(page).toHaveURL(/\/login/)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx playwright test e2e/specs/mobile-nav.spec.ts`
Expected: FAIL — no hamburger button exists; Dashboard link visible on 375px

- [ ] **Step 3: Write minimal implementation**

```tsx
// components/MobileNav.tsx
import { NavLink, useNavigate } from 'react-router-dom'
import { LogOut, Settings, Shield, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useAuthStore } from '../stores/auth'
import useOverlay from '../hooks/useOverlay'
import ThemeToggle from './ThemeToggle'
import LanguageSwitcher from './LanguageSwitcher'

const linkBase =
  'relative block rounded-md px-3 py-2.5 text-sm font-medium transition-colors duration-200'
const linkActive = 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)]'
const linkInactive =
  'text-[var(--color-text-secondary)] hover:bg-[var(--color-surface)] hover:text-[var(--color-text-primary)]'

interface MobileNavProps {
  open: boolean
  onClose: () => void
}

export default function MobileNav({ open, onClose }: MobileNavProps) {
  const user = useAuthStore((s) => s.user)
  const logout = useAuthStore((s) => s.logout)
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { overlayProps } = useOverlay(onClose)

  if (!open) return null

  const navLink = (to: string, label: string) => (
    <NavLink
      to={to}
      onClick={onClose}
      className={({ isActive }) => `${linkBase} ${isActive ? linkActive : linkInactive}`}
    >
      {label}
    </NavLink>
  )

  return (
    <>
      <div
        {...overlayProps}
        data-testid="mobile-nav-overlay"
        className="fixed inset-0 z-30 bg-black/30 backdrop-blur-sm"
      />
      <div
        className="glass-nav fixed inset-x-0 top-14 z-40 border-t px-4 pb-4 pt-2"
        style={{ borderColor: 'var(--color-border)' }}
      >
        <div className="flex items-center justify-between">
          <span
            className="text-xs font-medium uppercase tracking-wider"
            style={{ color: 'var(--color-text-muted)' }}
          >
            {user?.username}
          </span>
          <button
            onClick={onClose}
            aria-label={t('modal.close')}
            className="rounded p-1"
            style={{ color: 'var(--color-text-muted)' }}
          >
            <X className="h-5 w-5" />
          </button>
        </div>
        <div className="mt-2 flex flex-col gap-0.5">
          {navLink('/dashboard', t('nav.dashboard'))}
          {navLink('/gallery', t('nav.gallery'))}
          {user?.is_admin && navLink('/admin', t('nav.admin'))}
        </div>
        <div
          className="mt-3 flex items-center justify-between border-t pt-3"
          style={{ borderColor: 'var(--color-border)' }}
        >
          <div className="flex items-center gap-2">
            <ThemeToggle />
            <LanguageSwitcher />
          </div>
          <div className="flex items-center gap-1">
            <button
              onClick={() => { onClose(); navigate('/settings') }}
              className="flex items-center gap-1.5 rounded-md px-2.5 py-2 text-sm"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              <Settings className="h-4 w-4" /> {t('nav.settings')}
            </button>
            {user?.is_admin && (
              <button
                onClick={() => { onClose(); navigate('/admin') }}
                className="flex items-center gap-1.5 rounded-md px-2.5 py-2 text-sm"
                style={{ color: 'var(--color-text-secondary)' }}
              >
                <Shield className="h-4 w-4" /> {t('nav.admin')}
              </button>
            )}
            <button
              onClick={() => { logout(); navigate('/login', { replace: true }) }}
              className="flex items-center gap-1.5 rounded-md px-2.5 py-2 text-sm"
              style={{ color: 'var(--color-danger)' }}
            >
              <LogOut className="h-4 w-4" /> {t('nav.logout')}
            </button>
          </div>
        </div>
      </div>
    </>
  )
}
```

NavBar 改造要点（最小 diff）：
1. 链接组 `<div className="flex items-center gap-1">` → `<div className="hidden items-center gap-1 md:flex">`
2. 用户 pill 在 <md 隐藏用户名文字：`<span className="hidden max-w-[120px] truncate md:inline">{user?.username}</span>`；pill 宽度 `max-w-[180px]` → `max-w-[180px] md:max-w-none`
3. 汉堡按钮（放 Brand 与链接之间，<md 显示）：`<button className="md:hidden ..." aria-label={t('nav.menu')} onClick={() => setMobileOpen(true)}><Menu className="h-5 w-5" /></button>`（`Menu` 从 lucide-react 导入）
4. NavBar 内新增 `const [mobileOpen, setMobileOpen] = useState(false)` 与 `<MobileNav open={mobileOpen} onClose={() => setMobileOpen(false)} />`
5. DropdownMenu 用户菜单（Settings/Admin/Logout）外层包 `<div className="hidden md:block">`

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web-ui && npx playwright test e2e/specs/mobile-nav.spec.ts`
Expected: PASS (3 tests)

- [ ] **Step 5: Regression check + Commit**

```bash
npx playwright test e2e/specs/i18n.spec.ts e2e/specs/00-auth.spec.ts
git add web-ui/src/components/MobileNav.tsx web-ui/src/components/NavBar.tsx web-ui/e2e/specs/mobile-nav.spec.ts
git commit -m "feat: mobile hamburger navigation with drawer"
```

---

### Task 4: Add Sheet drawer + Gallery responsive pass

- **depends_on:** [T0, T1] | **breaking:** false

**Files:**
- Create: `web-ui/src/components/ui/Sheet.tsx`
- Modify: `web-ui/src/pages/Gallery.tsx`
- Test: `web-ui/e2e/specs/mobile-gallery.spec.ts`

**Interfaces:**
- Consumes: `useOverlay` (T0), `gallery.allCategories` key (T1), `CategoryTree` (existing)
- Produces: `Sheet({ open: boolean; onClose: () => void; title: string; children: ReactNode })` — left slide-in `w-[85vw] max-w-xs`
- Gallery 新增：`categorySheetOpen` state、工具栏"分类"按钮（`md:hidden`）、网格密度 `grid-cols-2 sm:grid-cols-3 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5`、选择按钮触屏常显

**回归红线：** `categories.spec.ts:96`（sidebar 渲染）、`gallery.spec.ts` 全量（搜索/排序/选择/删除）必须保持通过——≥md 侧栏与桌面网格不变。

- [ ] **Step 1: Write the failing E2E test**

```ts
// e2e/specs/mobile-gallery.spec.ts
import { test, expect } from '@playwright/test'
import { seedUserSession, ensureAuth } from '../helpers/auth'
import { createCategory } from '../helpers/api'

test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

test.describe.serial('mobile gallery', () => {
  test('category drawer opens and filters images', async ({ page, request }) => {
    await seedUserSession(page, request)
    const auth = await ensureAuth(request)
    const cat = await createCategory(request, auth.user.access_token, `mobile-cat-${Date.now()}`)
    await page.goto('/gallery')

    // Desktop sidebar hidden on mobile
    await expect(page.getByText(cat.name)).toHaveCount(0)

    // Open category drawer
    await page.getByRole('button', { name: /categories|分类/i }).click()
    await expect(page.getByText(cat.name)).toBeVisible()

    // Select the category → drawer closes
    await page.getByText(cat.name).click()
    await expect(page.getByText(cat.name)).toHaveCount(0)
    await expect(page).toHaveURL(/category_id=/)
  })
})
```

> 提示：`ensureAuth` 从 `../helpers/auth` 导入，`createCategory` 从 `../helpers/api` 导入（照抄 categories.spec.ts 用法）。

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx playwright test e2e/specs/mobile-gallery.spec.ts`
Expected: FAIL — no 分类 button on mobile

- [ ] **Step 3: Write minimal implementation**

```tsx
// ui/Sheet.tsx
import { type ReactNode } from 'react'
import { X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import useOverlay from '../../hooks/useOverlay'

interface SheetProps {
  open: boolean
  onClose: () => void
  title: string
  children: ReactNode
}

export default function Sheet({ open, onClose, title, children }: SheetProps) {
  const { t } = useTranslation()
  const { overlayProps } = useOverlay(onClose)
  if (!open) return null

  return (
    <>
      <div
        {...overlayProps}
        data-testid="sheet-overlay"
        className="fixed inset-0 z-40 bg-black/30 backdrop-blur-sm"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="glass-elevated fixed inset-y-0 left-0 z-50 flex w-[85vw] max-w-xs flex-col"
      >
        <div className="flex items-center justify-between px-4 py-3">
          <h2 className="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            {title}
          </h2>
          <button
            onClick={onClose}
            aria-label={t('modal.close')}
            className="rounded p-1"
            style={{ color: 'var(--color-text-muted)' }}
          >
            <X className="h-5 w-5" />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-4">{children}</div>
      </div>
    </>
  )
}
```

Gallery 改造要点：
1. 新增 `const [categorySheetOpen, setCategorySheetOpen] = useState(false)`
2. 工具栏（header 右侧 `flex items-center gap-2`）新增分类按钮（置于 SearchBar 前）：
```tsx
<button
  onClick={() => setCategorySheetOpen(true)}
  className="flex items-center gap-1.5 rounded-lg border border-[var(--glass-border-base)] bg-[var(--glass-tint-base)]/65 px-3 py-1.5 text-sm md:hidden"
  style={{ color: 'var(--color-text-secondary)' }}
>
  <Folder className="h-4 w-4" />
  {t('gallery.allCategories')}
</button>
```
（`Folder` 从 lucide-react 导入）
3. 页面末尾渲染：
```tsx
<Sheet
  open={categorySheetOpen}
  onClose={() => setCategorySheetOpen(false)}
  title={t('categoryTree.categories')}
>
  <CategoryTree
    selectedId={categoryFilter}
    onSelect={(id) => { setCategoryFilter(id); setCategorySheetOpen(false) }}
  />
</Sheet>
```
4. 网格：`grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5`
5. 选择按钮：`opacity-60 group-hover:opacity-100` → `opacity-100 md:opacity-60 md:group-hover:opacity-100`

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web-ui && npx playwright test e2e/specs/mobile-gallery.spec.ts`
Expected: PASS

- [ ] **Step 5: Regression check + Commit**

```bash
npx playwright test e2e/specs/gallery.spec.ts e2e/specs/categories.spec.ts
git add web-ui/src/components/ui/Sheet.tsx web-ui/src/pages/Gallery.tsx web-ui/e2e/specs/mobile-gallery.spec.ts
git commit -m "feat: mobile category drawer and responsive gallery grid"
```

---

### Task 5: Add CategoryTree touch menu + Modal swap

- **depends_on:** [T1, T2] | **breaking:** false

**Files:**
- Modify: `web-ui/src/components/CategoryTree.tsx`
- Test: `web-ui/e2e/specs/categories.spec.ts`（扩展移动端用例）

**Interfaces:**
- Consumes: `Modal` (T2), `categoryTree.moreActions` key (T1), `ConfirmDialog` (T2)
- Produces: CategoryTree 每行 ⋯ 按钮（触屏菜单入口，桌面 hover 显示）；右键 + 点击双触发同一菜单；两个 `w-80` 弹窗 → Modal/ConfirmDialog

**回归红线：** 现有 `categories.spec.ts` 全部用例 + 桌面侧栏渲染必须保持通过。

- [ ] **Step 1: Write the failing test** — append to `categories.spec.ts`（文件级 viewport 是桌面，故用独立 describe + test.use 覆盖）

```ts
test.describe('mobile category actions', () => {
  test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

  test('⋯ button opens menu; rename via touch path', async ({ page, request }) => {
    await seedUserSession(page, request)
    const cat = await createCategory(request, auth.user.access_token, `touch-rename-${Date.now()}`)
    await page.goto('/gallery')

    // Open drawer (sidebar hidden on mobile)
    await page.getByRole('button', { name: /categories|分类/i }).click()
    const row = page.getByText(cat.name).locator('xpath=../..')
    await row.getByRole('button', { name: /more actions|更多操作/i }).click()
    await page.getByText(/rename|重命名/i).click()
    // Inline rename input appears
    const input = page.locator('input[value="' + cat.name + '"]')
    await input.fill('renamed-touch')
    await input.press('Enter')
    await expect(page.getByText('renamed-touch')).toBeVisible()
  })

  test('delete confirm uses modal; cancel keeps category', async ({ page, request }) => {
    await seedUserSession(page, request)
    const cat = await createCategory(request, auth.user.access_token, `touch-del-${Date.now()}`)
    await page.goto('/gallery')
    await page.getByRole('button', { name: /categories|分类/i }).click()
    const row = page.getByText(cat.name).locator('xpath=../..')
    await row.getByRole('button', { name: /more actions|更多操作/i }).click()
    await page.getByText(/delete|删除/i).click()
    await expect(page.locator('.glass-modal')).toBeVisible()
    await page.locator('.glass-modal').getByRole('button', { name: /cancel|取消/i }).click()
    await expect(page.getByText(cat.name)).toBeVisible()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx playwright test e2e/specs/categories.spec.ts`
Expected: FAIL — no 更多操作 button on rows

- [ ] **Step 3: Write minimal implementation**

CategoryTree 改造要点：
1. `TreeNode` 每行追加 ⋯ 按钮（在名字 `span` 后）：
```tsx
<button
  onClick={(e) => { e.stopPropagation(); onMenuButtonClick(e, node.id, node.name) }}
  aria-label={t('categoryTree.moreActions')}
  className="rounded p-1 opacity-60 transition-opacity hover:opacity-100 md:opacity-0 md:group-hover:opacity-100"
  style={{ color: 'var(--color-text-muted)' }}
>
  <MoreHorizontal size={14} />
</button>
```
（`MoreHorizontal` 从 lucide-react 导入；`t` 在 TreeNode 内通过 `useTranslation()` 获取）
2. `ContextMenuState` 扩展：`{ x, y, nodeId, nodeName, anchor?: 'cursor' | 'button', buttonRect?: DOMRect }`；新增 `handleMenuButtonClick(e, nodeId, nodeName)`：`e.currentTarget.getBoundingClientRect()` 定位（x = rect.left, y = rect.bottom + 4, anchor='button'）
3. 菜单定位钳制：`const menuX = Math.min(state.x, window.innerWidth - 160)`、`menuY = Math.min(state.y, window.innerHeight - 100)`（160 = min-w-[130px] + 边距）
4. 创建/删除弹窗 → 共享组件：
   - 创建：`<Modal open={showCreate} onClose={...} title={t('categoryTree.newCategory')} size="sm">` 包裹原 input + 按钮（替换 `createPortal` + `w-80`）
   - 删除：`<ConfirmDialog open={!!deleteConfirmId} onClose={() => setDeleteConfirmId(null)} onConfirm={() => deleteMutation.mutate(deleteConfirmId!)} title={t('categoryTree.deleteCategory')} message={t('categoryTree.deleteConfirm')} confirmLabel={t('categoryTree.delete')} danger pending={deleteMutation.isPending} />`
5. 删除 `createPortal` 使用（Modal/ConfirmDialog 内部已处理）；保留原 onKeyDown Enter/Escape 逻辑于 Modal children 中

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web-ui && npx playwright test e2e/specs/categories.spec.ts`
Expected: PASS（现有 + 新增 2 个移动端用例）

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/components/CategoryTree.tsx web-ui/e2e/specs/categories.spec.ts
git commit -m "feat: touch-friendly category menu and shared modal dialogs"
```

---

### Task 6: Migrate EditUserDialog + CreateInviteDialog to Modal

- **depends_on:** [T2] | **breaking:** false

**Files:**
- Modify: `web-ui/src/components/EditUserDialog.tsx`
- Modify: `web-ui/src/components/CreateInviteDialog.tsx`
- Test: `web-ui/e2e/specs/admin.spec.ts`（扩展移动端 viewport 用例）

**Interfaces:**
- Consumes: `Modal` (T2)
- Produces: 两个对话框保持相同 props（`user/onClose/onUpdated`；`onClose/onCreated`），内部骨架换为 Modal

**回归红线：** `admin.spec.ts:49`（edit user dialog opens and closes）、`:69`（invites tab creates）、`:76`（`.glass-modal` 定位）必须保持通过。

- [ ] **Step 1: Write the failing test** — append to `admin.spec.ts`

```ts
test.describe('admin dialogs on mobile', () => {
  test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

  test('edit user dialog renders as bottom sheet on mobile', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/admin')
    await page.getByRole('tab', { name: /users|用户/i }).click()
    // Open edit dialog (first pencil button)
    await page.locator('button').filter({ has: page.locator('svg.lucide-pencil') }).first().click()
    const panel = page.locator('.glass-modal')
    await expect(panel).toBeVisible()
    // Bottom-sheet: panel bottom-aligned, full width on small screens
    const box = await panel.boundingBox()
    const vh = page.viewportSize()!.height
    expect(box!.y + box!.height).toBeGreaterThan(vh - 100)
    await page.keyboard.press('Escape')
    await expect(panel).toBeHidden()
  })

  test('create invite dialog opens on mobile', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/admin')
    await page.getByRole('tab', { name: /invites|邀请/i }).click()
    await page.getByRole('button', { name: /create code|创建邀请/i }).click()
    await expect(page.locator('.glass-modal')).toBeVisible()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx playwright test e2e/specs/admin.spec.ts`
Expected: FAIL — 现状弹窗 `items-center` 居中，底部对齐断言不通过

- [ ] **Step 3: Write minimal implementation**

EditUserDialog 改造：外层 `<div className="fixed inset-0 z-50 flex items-center justify-center p-4">…</div>` 整体替换为 `<Modal open onClose={onClose} title={t('editUser.title')} footer={…}>`；`<form>` 内容（字段 + 保存按钮）保留在 children 与 footer 中。同样处理 CreateInviteDialog（两阶段：表单阶段 + 成功阶段各一个 Modal 或同一 Modal 内切换内容）。

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web-ui && npx playwright test e2e/specs/admin.spec.ts`
Expected: PASS（现有 + 新增移动端用例）

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/components/EditUserDialog.tsx web-ui/src/components/CreateInviteDialog.tsx web-ui/e2e/specs/admin.spec.ts
git commit -m "refactor: admin dialogs use shared Modal (bottom sheet on mobile)"
```

---

### Task 7: Migrate StorageConfigSection dialogs to Modal

- **depends_on:** [T2] | **breaking:** false

**Files:**
- Modify: `web-ui/src/components/StorageConfigSection.tsx`
- Test: `web-ui/e2e/specs/settings.spec.ts`（扩展）

**Interfaces:**
- Consumes: `Modal`、`ConfirmDialog` (T2)
- Produces: ConfigModal/DeleteConfirm → Modal

**回归红线：** `settings.spec.ts:76`（storage backends add modal opens）、`:88`（creating without token rejected）必须保持通过。

- [ ] **Step 1: Write the failing test** — append to `settings.spec.ts`

```ts
test.describe('storage config dialogs on mobile', () => {
  test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

  test('config modal is bottom sheet on mobile', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/settings')
    await page.getByRole('button', { name: /storage backends|存储后端/i }).click()
    await page.getByRole('button', { name: /add|添加/i }).click()
    const panel = page.locator('.glass-modal')
    await expect(panel).toBeVisible()
    const box = await panel.boundingBox()
    const vh = page.viewportSize()!.height
    expect(box!.y + box!.height).toBeGreaterThan(vh - 100)
    await page.keyboard.press('Escape')
    await expect(panel).toBeHidden()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx playwright test e2e/specs/settings.spec.ts`
Expected: FAIL — 居中弹窗底部对齐断言失败

- [ ] **Step 3: Write minimal implementation**

StorageConfigSection：`ConfigModal`（L149-153 portal）与 `DeleteConfirm`（L378-382 portal）改用 `Modal`/`ConfirmDialog`，保留内部表单/回调。

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web-ui && npx playwright test e2e/specs/settings.spec.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/components/StorageConfigSection.tsx web-ui/e2e/specs/settings.spec.ts
git commit -m "refactor: storage config dialogs use shared Modal"
```

---

### Task 8: Migrate SystemConfig confirm to ConfirmDialog

- **depends_on:** [T2] | **breaking:** false

**Files:**
- Modify: `web-ui/src/components/SystemConfig.tsx`
- Test: `web-ui/e2e/specs/admin.spec.ts`（扩展 confirm 断言）

**Interfaces:**
- Consumes: `ConfirmDialog` (T2)
- Produces: SystemConfig restore `window.confirm` → ConfirmDialog

**回归红线：** `admin.spec.ts` system config 用例（`:84` view masked secrets、`:111` backup and restore）保持通过。

- [ ] **Step 1: Write the failing test** — append to `admin.spec.ts`

```ts
test.describe('system config confirm on mobile', () => {
  test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

  test('restore triggers ConfirmDialog instead of native confirm', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/admin')
    await page.getByRole('tab', { name: /config|配置/i }).click()
    // Open backups section and click restore on first backup
    await page.getByRole('button', { name: /restore|恢复/i }).first().click()
    await expect(page.locator('.glass-modal')).toBeVisible()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx playwright test e2e/specs/admin.spec.ts`
Expected: FAIL — native confirm 无 DOM 断言（.glass-modal 不存在）

- [ ] **Step 3: Write minimal implementation**

SystemConfig：`window.confirm(...)`（L237）→ `ConfirmDialog` state（`restoreConfirmOpen` + 待恢复文件名），确认后执行原 restore 逻辑。

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web-ui && npx playwright test e2e/specs/admin.spec.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/components/SystemConfig.tsx web-ui/e2e/specs/admin.spec.ts
git commit -m "refactor: system config restore uses shared ConfirmDialog"
```

---

### Task 9: Migrate Gallery dialogs to ConfirmDialog + batch-move toast

- **depends_on:** [T2, T4] | **breaking:** false

**Files:**
- Modify: `web-ui/src/pages/Gallery.tsx`
- Test: `web-ui/e2e/specs/gallery.spec.ts`（扩展）

**Interfaces:**
- Consumes: `ConfirmDialog` (T2)
- Produces: batch-delete confirm → ConfirmDialog；batch-move `alert()` → sonner toast

**回归红线：** `gallery.spec.ts:70`（batch delete removes images，用 `gallery.po.ts` 的 `deleteButton/confirmDeleteButton`）必须保持通过——页面对象选择器若依赖旧结构需同步更新。

- [ ] **Step 1: Write the failing test** — append to `gallery.spec.ts`

```ts
test('batch delete confirm renders as shared Modal', async ({ page, request }) => {
  await seedUserSession(page, request)
  await page.goto('/gallery')
  // Enter select mode via first tile's select button
  await page.locator('button[aria-label*="select"]').first().click()
  await page.getByRole('button', { name: /delete|删除/i }).click()
  await expect(page.locator('.glass-modal')).toBeVisible()
  await page.locator('.glass-modal').getByRole('button', { name: /cancel|取消/i }).click()
  await expect(page.locator('.glass-modal')).toBeHidden()
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx playwright test e2e/specs/gallery.spec.ts`
Expected: FAIL — 当前无 `.glass-modal` 或选择器不匹配

- [ ] **Step 3: Write minimal implementation**

Gallery 改造：`showConfirm` 弹窗 → `<ConfirmDialog open={showConfirm} onClose={() => setShowConfirm(false)} onConfirm={confirmDelete} title={t('gallery.deleteConfirm', { count: selected.size })} message={...} confirmLabel={t('gallery.delete')} danger pending={isDeleting} />`；`handleBatchMove` 的 `alert(...)` → `toast.info(t('gallery.batchMovePlaceholder', { count: selected.size }))`（需 `import { toast } from 'sonner'`）。同步更新 `gallery.po.ts` 若选择器变化。

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web-ui && npx playwright test e2e/specs/gallery.spec.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/pages/Gallery.tsx web-ui/e2e/specs/gallery.spec.ts
git commit -m "feat: gallery batch dialogs use shared ConfirmDialog; batch-move toast"
```

---

### Task 10: Cardify Admin tables for mobile

- **depends_on:** [T1, T2, T6] | **breaking:** false

**Files:**
- Modify: `web-ui/src/pages/admin/AdminUsers.tsx`
- Modify: `web-ui/src/pages/admin/AdminInvites.tsx`
- Test: `web-ui/e2e/specs/mobile-admin.spec.ts`

**Interfaces:**
- Consumes: `ConfirmDialog` (T2), `EditUserDialog` (existing, now Modal-backed), `adminUsers.deleteTitle` key (T1)
- Produces: 两表 `<sm` 卡片列表（`sm:hidden`）+ ≥sm 表格（`hidden sm:block` + `overflow-x-auto`）；AdminUsers 原生 `confirm()` → ConfirmDialog

**回归红线：** `admin.spec.ts:39`（users tab lists users）、`:69`（invites tab creates）必须保持通过——桌面表格结构不变。

- [ ] **Step 1: Write the failing test**

```ts
// e2e/specs/mobile-admin.spec.ts
import { test, expect } from '@playwright/test'
import { seedUserSession } from '../helpers/auth'

test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

test.describe.serial('mobile admin', () => {
  test('users render as cards on mobile', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/admin')
    await page.getByRole('tab', { name: /users|用户/i }).click()
    // Table hidden, cards visible
    await expect(page.locator('table')).toHaveCount(0)
    await expect(page.locator('[data-testid="user-card"]').first()).toBeVisible()
  })

  test('delete user opens ConfirmDialog on mobile', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/admin')
    await page.getByRole('tab', { name: /users|用户/i }).click()
    const card = page.locator('[data-testid="user-card"]').first()
    await card.getByRole('button', { name: /delete|删除/i }).click()
    await expect(page.locator('.glass-modal')).toBeVisible()
  })

  test('invites render as cards on mobile', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/admin')
    await page.getByRole('tab', { name: /invites|邀请/i }).click()
    await expect(page.locator('table')).toHaveCount(0)
    await expect(page.locator('[data-testid="invite-card"]').first()).toBeVisible()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx playwright test e2e/specs/mobile-admin.spec.ts`
Expected: FAIL — table still rendered; no card elements

- [ ] **Step 3: Write minimal implementation**

AdminUsers：
1. 表格容器 `<div className="glass overflow-hidden rounded-xl">` → `<div className="glass hidden overflow-x-auto rounded-xl sm:block">`（`overflow-hidden` → `overflow-x-auto`）
2. 用户名单元格加 `truncate max-w-[180px]`
3. 新增卡片容器 `<div className="mt-3 flex flex-col gap-2 sm:hidden">`：
```tsx
{data.users.map((user) => (
  <div key={user.id} data-testid="user-card" className="glass rounded-xl p-4">
    <div className="flex items-center justify-between gap-2">
      <span className="truncate text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
        {user.username}
      </span>
      {user.is_admin && (
        <span
          className="badge shrink-0"
          style={{ backgroundColor: 'var(--color-accent-subtle)', color: 'var(--color-accent)', borderColor: 'var(--color-accent-strong)' }}
        >
          {t('adminUsers.adminBadge')}
        </span>
      )}
    </div>
    <div className="mt-1 truncate text-xs" style={{ color: 'var(--color-text-secondary)' }}>
      {user.email || '—'}
    </div>
    <div className="mt-1 font-mono text-xs" style={{ color: 'var(--color-text-secondary)' }}>
      {user.storage_quota != null ? formatBytes(user.storage_quota) : t('adminUsers.unlimited')}
    </div>
    <div className="mt-3 flex justify-end gap-2">
      <button
        onClick={() => setEditingUser(user)}
        className="flex min-h-[44px] items-center gap-1.5 rounded-lg px-3 text-sm"
        style={{ color: 'var(--color-text-secondary)' }}
      >
        <Pencil className="h-4 w-4" /> {t('editUser.title')}
      </button>
      <button
        onClick={() => setDeleteUser(user)}
        className="flex min-h-[44px] items-center gap-1.5 rounded-lg px-3 text-sm"
        style={{ color: 'var(--color-danger)' }}
      >
        <Trash2 className="h-4 w-4" /> {t('common.delete')}
      </button>
    </div>
  </div>
))}
```
4. 原生 confirm → `const [deleteUser, setDeleteUser] = useState<UserInfo | null>(null)` + 页面底部：
```tsx
<ConfirmDialog
  open={!!deleteUser}
  onClose={() => setDeleteUser(null)}
  onConfirm={() => deleteUser && handleDelete(deleteUser)}
  title={t('adminUsers.deleteTitle')}
  message={t('adminUsers.deleteConfirm', { name: deleteUser?.username ?? '' })}
  confirmLabel={t('common.delete')}
  danger
/>
```
（`handleDelete` 内去掉 `if (!confirm(...))` 分支）

AdminInvites 同理：`[data-testid="invite-card"]` 卡片显示 code（truncateCode）/创建时间/过期时间/状态徽章/复制按钮；表格容器 `hidden sm:block` + `overflow-x-auto`。

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web-ui && npx playwright test e2e/specs/mobile-admin.spec.ts e2e/specs/admin.spec.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/pages/admin/AdminUsers.tsx web-ui/src/pages/admin/AdminInvites.tsx web-ui/e2e/specs/mobile-admin.spec.ts
git commit -m "feat: admin tables cardified for mobile; confirm dialogs"
```

---

### Task 11: Add global overflow guard + container paddings

- **depends_on:** [T3] | **breaking:** false

**Files:**
- Modify: `web-ui/src/index.css`
- Modify: `web-ui/src/components/Layout.tsx`
- Test: `web-ui/e2e/specs/responsive.spec.ts`

**Interfaces:**
- Produces: `html, body { overflow-x: clip }`；`Layout` main `px-4 sm:px-6`；NavBar `px-4 sm:px-5`（NavBar 在 T3 已改，此处仅 padding 微调，避免冲突——若 T3 已改则跳过）

- [ ] **Step 1: Write the failing test**

```ts
// e2e/specs/responsive.spec.ts
import { test, expect } from '@playwright/test'
import { seedUserSession } from '../helpers/auth'

const PAGES = ['/dashboard', '/gallery', '/settings', '/admin']

test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

test.describe.serial('no horizontal overflow on mobile', () => {
  for (const path of PAGES) {
    test(`${path} has no horizontal scroll`, async ({ page, request }) => {
      await seedUserSession(page, request)
      await page.goto(path)
      await page.waitForTimeout(300)
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      )
      expect(overflow).toBeLessThanOrEqual(1)
    })
  }
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx playwright test e2e/specs/responsive.spec.ts`
Expected: FAIL — 至少一个页面出现横向滚动

- [ ] **Step 3: Write minimal implementation**

```css
/* index.css base 层追加 */
html,
body {
  overflow-x: clip;
}
```

```tsx
// Layout.tsx
<main className="mx-auto max-w-5xl px-4 py-6 sm:px-6" ...>
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web-ui && npx playwright test e2e/specs/responsive.spec.ts`
Expected: PASS（4 页面均无横向溢出）

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/index.css web-ui/src/components/Layout.tsx web-ui/e2e/specs/responsive.spec.ts
git commit -m "feat: global overflow guard and responsive container paddings"
```

---

### Task 12: Add responsive settings grids

- **depends_on:** [T11] | **breaking:** false

**Files:**
- Modify: `web-ui/src/components/WatermarkSettings.tsx`
- Modify: `web-ui/src/components/PreprocessingSettings.tsx`
- Test: `web-ui/e2e/specs/responsive.spec.ts`（扩展 `/settings` 深层检查）

**Interfaces:**
- Consumes: 无（纯类名）
- Produces: WatermarkSettings `grid-cols-1 sm:grid-cols-2`；PreprocessingSettings 行 `flex-wrap`

- [ ] **Step 1: Write the failing test** — append to `responsive.spec.ts`

```ts
test('/settings expanded sections fit without overflow', async ({ page, request }) => {
  await seedUserSession(page, request)
  await page.goto('/settings#settings?section=watermark')
  await page.waitForTimeout(300)
  await page.goto('/settings#settings?section=preprocessing')
  await page.waitForTimeout(300)
  const overflow = await page.evaluate(
    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
  )
  expect(overflow).toBeLessThanOrEqual(1)
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx playwright test e2e/specs/responsive.spec.ts`
Expected: FAIL — 水印双列网格在小屏溢出

- [ ] **Step 3: Write minimal implementation**

```tsx
// WatermarkSettings.tsx:211
<div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
// PreprocessingSettings: 含 w-20/w-32/w-40 的行容器补 flex-wrap
<div className="flex flex-wrap items-center gap-3">
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web-ui && npx playwright test e2e/specs/responsive.spec.ts e2e/specs/settings.spec.ts`
Expected: PASS（新增断言 + settings 现有用例回归）

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/components/WatermarkSettings.tsx web-ui/src/components/PreprocessingSettings.tsx web-ui/e2e/specs/responsive.spec.ts
git commit -m "feat: responsive settings grids and wrapping"
```

---

### Task 13: Add DropZone touch polish + Admin tabs scroll

- **depends_on:** [T11] | **breaking:** false

**Files:**
- Modify: `web-ui/src/components/DropZone.tsx`
- Modify: `web-ui/src/pages/Admin.tsx`
- Test: `web-ui/e2e/specs/upload.spec.ts`（扩展移动端用例）

**Interfaces:**
- Consumes: 无
- Produces: DropZone 点击区 `min-h-[140px]`；Admin 标签栏 `overflow-x-auto`

- [ ] **Step 1: Write the failing test** — append to `upload.spec.ts`

```ts
test.describe('dropzone on mobile', () => {
  test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

  test('dropzone has adequate tap height', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/dashboard')
    const zone = page
      .getByText(/drag|drop|拖拽|选择/i)
      .first()
      .locator('xpath=ancestor::div[contains(@class,"glass")][1]')
    const box = await zone.boundingBox()
    expect(box!.height).toBeGreaterThanOrEqual(140)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx playwright test e2e/specs/upload.spec.ts`
Expected: 若当前高度 <140px 则 FAIL；若已达标则 PASS（防御性回归，非阻塞）

- [ ] **Step 3: Write minimal implementation**

```tsx
// DropZone.tsx root className 追加
className={`glass group relative flex min-h-[140px] cursor-pointer items-center justify-center overflow-hidden rounded-xl border-2 border-dashed p-8 text-center transition-all duration-300 sm:p-12 ${...}`}
```

```tsx
// Admin.tsx 标签栏
<div className="glass mb-5 flex gap-0.5 overflow-x-auto rounded-lg p-1">
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web-ui && npx playwright test e2e/specs/upload.spec.ts e2e/specs/responsive.spec.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/components/DropZone.tsx web-ui/src/pages/Admin.tsx web-ui/e2e/specs/upload.spec.ts
git commit -m "feat: dropzone mobile tap area and admin tabs scroll"
```

---

### Task 14: Add popover viewport clamping (GlassSelect + DropdownMenu)

- **depends_on:** [] | **breaking:** false

**Files:**
- Modify: `web-ui/src/components/ui/GlassSelect.tsx`
- Modify: `web-ui/src/components/ui/DropdownMenu.tsx`
- Test: `web-ui/src/components/ui/popover-clamp.test.ts`（vitest 纯函数）

**Interfaces:**
- Produces: `clampLeft(left: number, width: number, viewportWidth: number): number`（从 GlassSelect 导出纯函数便于单测）——右缘超视口时回退，最小 8px 边距

- [ ] **Step 1: Write the failing test**

```ts
// ui/popover-clamp.test.ts
import { describe, it, expect } from 'vitest'
import { clampLeft } from './GlassSelect'

describe('clampLeft', () => {
  it('returns left unchanged when it fits', () => {
    expect(clampLeft(100, 200, 375)).toBe(100)
  })
  it('clamps when right edge exceeds viewport', () => {
    expect(clampLeft(300, 200, 375)).toBe(167) // 300+200=500 > 375-8 → maxLeft=167
  })
  it('never goes below 8px margin', () => {
    expect(clampLeft(-50, 200, 375)).toBe(8)
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx vitest run src/components/ui/popover-clamp.test.ts`
Expected: FAIL — `clampLeft` not exported

- [ ] **Step 3: Write minimal implementation**

```ts
// GlassSelect.tsx 导出
export function clampLeft(left: number, width: number, viewportWidth: number): number {
  const margin = 8
  const maxLeft = viewportWidth - width - margin
  return Math.max(margin, Math.min(left, maxLeft))
}
```
应用：`GlassSelect` `updatePosition` 中 `left: clampLeft(rect.left, rect.width, window.innerWidth)`；listbox 加 `maxWidth: 'calc(100vw - 16px)'`。`DropdownMenu` 面板加 `max-w-[calc(100vw-2rem)]` 类（`right: 0` 对齐已安全；`left: 0` 场景超出视口时 clamp）。

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd web-ui && npx vitest run src/components/ui/popover-clamp.test.ts && npx vitest run`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/components/ui/GlassSelect.tsx web-ui/src/components/ui/DropdownMenu.tsx web-ui/src/components/ui/popover-clamp.test.ts
git commit -m "feat: clamp popovers to viewport on small screens"
```

---

### Task 15: Make ImageDetail rename affordance touch-visible

- **depends_on:** [] | **breaking:** false

**Files:**
- Modify: `web-ui/src/pages/ImageDetail.tsx`
- Test: `web-ui/e2e/specs/image-detail.spec.ts`（扩展移动端用例）

**Interfaces:**
- Consumes: 无
- Produces: 重命名铅笔 <md 常显（`md:opacity-0 md:group-hover:opacity-100`）

**回归红线：** 现有 `image-detail.spec.ts` 全部用例保持通过。

- [ ] **Step 1: Write the failing test** — append to `image-detail.spec.ts`

```ts
test.describe('image detail on mobile', () => {
  test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

  test('rename pencil visible without hover on touch', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/gallery')
    await page.locator('.glass img').first().click()
    await expect(page.locator('svg.lucide-pencil')).toBeVisible()
  })
})
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cd web-ui && npx playwright test e2e/specs/image-detail.spec.ts`
Expected: FAIL — pencil has `opacity-0`（不可见）

- [ ] **Step 3: Write minimal implementation**

```tsx
// ImageDetail.tsx:204
<Pencil className="h-3 w-3 transition-opacity md:opacity-0 md:group-hover:opacity-100" />
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cd web-ui && npx playwright test e2e/specs/image-detail.spec.ts`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add web-ui/src/pages/ImageDetail.tsx web-ui/e2e/specs/image-detail.spec.ts
git commit -m "feat: always-visible rename affordance on touch devices"
```

---

### Task 16: Bump version to 0.19.0

- **depends_on:** [T0-T15 全部] | **breaking:** false

**Files:**
- Modify: `Cargo.toml`
- Modify: `web-ui/package.json`
- Modify: `CHANGELOG.md`

**Interfaces:** 无

- [ ] **Step 1: Bump versions** — `0.18.0` → `0.19.0`（Cargo.toml workspace 版本 + web-ui/package.json `version` 字段；CHANGELOG 顶部新增 `## [0.19.0] - 2026-08-08` 条目，描述响应式布局特性列表）

- [ ] **Step 2: Verify**

Run: `grep -n '"version"' web-ui/package.json && grep -n '^version' Cargo.toml && cd web-ui && npm run build`
Expected: 均显示 `0.19.0`；build 通过

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml web-ui/package.json CHANGELOG.md
git commit -m "chore: bump version to 0.19.0 for responsive layout feature"
```

---

## Final Verification

```bash
cd web-ui && npm run build          # tsc -b && vite build — 全量类型检查
cd web-ui && npx vitest run          # 全部单测（含新增 useOverlay/Modal/clamp/i18n）
cd web-ui && npx playwright test     # 全部 E2E（73 现有 + 新增 mobile/responsive specs；需 Docker PG+Redis）
cargo clippy --workspace -- -D warnings  # 确认 Rust 侧无回归
```

## Post-Phase Docs Sync（AGENTS.md 规则要求，在 plan 验收后执行）

1. `AGENTS.md`：Frontend 章节新增响应式布局架构说明（MobileNav/Sheet/Modal 组件、useOverlay hook、断点策略、卡片化表格）
2. `README.md`：Features 清单新增响应式布局条目；Project Structure 更新组件列表
3. `.omo/summary/summary_and_next.md`：新增 "## 响应式布局 ✅" 章节，记录实现与验证结果，更新待实施表
4. Commit：`docs: auto-sync AGENTS.md, README.md, summary after responsive layout completion`

## Self-Review 记录（plan-validator PASS WITH ADVISORY 修订后）

- **规格覆盖**：§3 全局断点（T4/T11/T12/T13）、§4 MobileNav（T3）、§5 Sheet+CategoryTree（T4/T5）、§6 Modal+替换清单（T2/T5/T6/T7/T8/T9）、§7 表格卡片化（T10）、§8 触屏可用性（T13/T15）、§9 测试（各任务内嵌）、§11 版本（T16）——无缺口
- **advisory 修订**：
  - T7/T10 拆分（原 4 文件 → 每任务 ≤3 文件）✅
  - T4 补 `ensureAuth` 导入 ✅
  - T9 补 `adminUsers.deleteTitle` key（加入 T1 键集）✅
  - T0/T2/T14 全量 vitest 回归补入 verify ✅
  - 标题改祈使句 ✅
- **占位符扫描**：无 TBD/TODO；`gallery.po.ts` 选择器同步（T9）、`Admin.tsx` 标签栏与 `DropZone` 合并任务（T13）为明确标注的实现细节
- **类型一致性**：`Modal` props（open/onClose/title/children/footer/size）、`ConfirmDialog` props、`Sheet` props、`MobileNav` props、`useOverlay` 返回签名、`clampLeft` 签名在各任务 Interfaces 块一致声明
