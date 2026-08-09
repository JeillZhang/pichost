# 图片详情页缩放查看器 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Tasks MUST be executed in schema order (T0 → T5); within each task, write `test_code` first, verify it FAILS, then write `impl_code`, verify it PASSES, then commit. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 图片详情页支持放大/缩小查看图片 — 点击图片打开全屏 Lightbox 查看器，支持滚轮/按钮/键盘缩放、拖拽/单指平移、双击 fit↔100%、双指捏合。

**Architecture:** 新增纯逻辑 hook `useImageZoom`（scale/offset 状态 + 锚点缩放数学 + 钳制，无 DOM 依赖）与展示组件 `ImageViewer`（useOverlay 覆盖层 + Pointer Events 手势 + 玻璃工具栏）。ImageDetail 仅加 open state + 卡片点击 + 挂载。i18n 新增 6 个 `imageDetail.zoom*` 键（en/zh-CN 双写）。任务顺序：T0 hook → T1 i18n 键 → T2 组件 → T3 集成 → T4 版本 → T5 文档（T1 必须先于 T2，因为 `npm run build` 的 tsc -b 以 en.json 为 t() 键的类型源）。

**Tech Stack:** React 19, TypeScript 7, Tailwind CSS 4, lucide-react, react-i18next, Vitest (createRoot+act+dispatchEvent, 无 testing-library), Playwright E2E。

**设计文档:** `docs/superpowers/specs/2026-08-09-pichost-image-zoom-design.md`（已批准，本计划唯一规格依据）

## Global Constraints

- 纯前端任务 — 不允许任何 Rust/后端改动（0 crate 变更）
- 缩放范围 `[fitScale, 8.0]`；`fitScale = min(vw/nw, vh/nh, 1)`（小图不放大）；`scale 1.0 = 100%` 自然像素
- 滚轮步进 ×1.1，按钮步进 ×1.25；锚点公式 `offset' = anchor - (anchor - offset) × (newScale/oldScale)`
- 关闭即重置 fit，不记忆状态
- i18n 键必须 en/zh-CN 同步（`i18n.test.ts` 键集相等测试兜底）
- 禁止改动现有 E2E 选择器：`button:has(.lucide-pencil)`、`getByRole('combobox', { name: 'Category' })`、`code` 元素序、back 按钮
- 版本 bump：`0.19.1 → 0.20.0`（仅 workspace 根 `Cargo.toml` + `web-ui/package.json`，crates 均 `version.workspace = true`）
- 不引入新 npm 依赖
- 提交风格：semantic commits，English
- 每个任务的 `test_code` 必须先于 `impl_code` 编写，并先验证失败（TDD）

---

### Task T0: Add useImageZoom hook with anchored zoom math and clamping

**Files:**
- Create: `web-ui/src/hooks/useImageZoom.ts`
- Create: `web-ui/src/hooks/useImageZoom.test.ts`

**Interfaces:**
- Produces: `useImageZoom()` → `{ zoom: ZoomState, open(naturalW, naturalH, viewportW, viewportH), zoomAt(factor, anchorX, anchorY), zoomBy(factor), panBy(dx, dy), toggleFit(), reset(), isFit, displayPercent }`；常量 `MAX_ZOOM = 8`, `WHEEL_STEP = 1.1`, `BUTTON_STEP = 1.25`。坐标约定：anchor/offset 均为**相对视口中心**的像素坐标。

```yaml
- id: T0
  title: "Add useImageZoom hook with anchored zoom math and clamping"
  files:
    - Create: web-ui/src/hooks/useImageZoom.ts
    - Create: web-ui/src/hooks/useImageZoom.test.ts
  depends_on: []
  breaking: false
  ac:
    - given: "a freshly mounted useImageZoom"
      when: "open(1000, 500, 500, 500) is called"
      then: "zoom.scale and fitScale equal 0.5 with offsetX/offsetY 0; open(100, 100, 1000, 800) yields fitScale 1 (never upscales)"
    - given: "scale at fitScale 0.5"
      when: "zoomBy(1000) then zoomBy(0.0001) are called"
      then: "scale clamps to MAX_ZOOM (8) then back to fitScale"
    - given: "scale 0.5, offset (0, 0)"
      when: "zoomAt(2, 100, 50) is called"
      then: "scale becomes 1.0 and offset (-100, -50) — the image point under the anchor is invariant"
    - given: "scale 2.0 (1000×1000 image in 500×500 viewport)"
      when: "panBy(10000, -10000) then panBy(-10000, -10000) are called"
      then: "offset clamps to ±750; at fit scale panning is disabled (max 0)"
    - given: "a zoomed/panned state"
      when: "toggleFit() or reset() is called"
      then: "scale returns to fitScale; toggleFit from fit sets scale 1.0; reset zeroes offset"
    - given: "any state"
      when: "displayPercent and isFit are read"
      then: "displayPercent equals Math.round(scale*100) and isFit is true only at fitScale"
  regression:
    - "cd web-ui && npx vitest run src/hooks/useOverlay.test.tsx"
  test_code: |
    import { describe, it, expect } from 'vitest'
    import { act } from 'react'
    import { createRoot, type Root } from 'react-dom/client'
    import { useImageZoom, MAX_ZOOM } from './useImageZoom'

    type ZoomApi = ReturnType<typeof useImageZoom>

    function Harness({ onReady }: { onReady: (api: ZoomApi) => void }) {
      const api = useImageZoom()
      onReady(api) // reassigned on every render → latest state after act()
      return null
    }

    function mount(): { api: ZoomApi; root: Root } {
      let api: ZoomApi = null as unknown as ZoomApi
      const container = document.createElement('div')
      document.body.appendChild(container)
      const root = createRoot(container)
      act(() => root.render(<Harness onReady={(a) => { api = a }} />))
      return { api, root }
    }

    describe('useImageZoom', () => {
      it('open computes contain fitScale and never upscales small images', () => {
        const { api, root } = mount()
        act(() => api.open(1000, 500, 500, 500))
        expect(api.zoom.scale).toBe(0.5)
        expect(api.zoom.fitScale).toBe(0.5)
        expect(api.zoom.offsetX).toBe(0)
        act(() => api.open(100, 100, 1000, 800))
        expect(api.zoom.fitScale).toBe(1) // min(10, 8, 1)
        act(() => root.unmount())
      })

      it('open guards against zero natural size', () => {
        const { api, root } = mount()
        act(() => api.open(0, 0, 500, 500))
        expect(api.zoom.fitScale).toBe(1)
        act(() => root.unmount())
      })

      it('clamps zoom to [fitScale, MAX_ZOOM]', () => {
        const { api, root } = mount()
        act(() => api.open(1000, 1000, 500, 500))
        act(() => api.zoomBy(1000))
        expect(api.zoom.scale).toBe(MAX_ZOOM)
        act(() => api.zoomBy(0.0001))
        expect(api.zoom.scale).toBe(api.zoom.fitScale)
        act(() => root.unmount())
      })

      it('zoomAt keeps the image point under the anchor fixed', () => {
        const { api, root } = mount()
        act(() => api.open(1000, 1000, 500, 500))
        act(() => api.zoomAt(2, 100, 50))
        expect(api.zoom.scale).toBe(1)
        expect(api.zoom.offsetX).toBe(-100) // 100 - (100 - 0) * 2
        expect(api.zoom.offsetY).toBe(-50) // 50 - (50 - 0) * 2
        act(() => root.unmount())
      })

      it('zoomBy keeps the viewport center fixed', () => {
        const { api, root } = mount()
        act(() => api.open(1000, 1000, 500, 500))
        act(() => api.zoomAt(2, 100, 50))
        act(() => api.zoomBy(2))
        expect(api.zoom.offsetX).toBe(-200) // -100 * 2
        expect(api.zoom.offsetY).toBe(-100)
        act(() => root.unmount())
      })

      it('panBy clamps to image bounds and disables panning at fit', () => {
        const { api, root } = mount()
        act(() => api.open(1000, 1000, 500, 500))
        act(() => api.panBy(10000, 0))
        expect(api.zoom.offsetX).toBe(0) // at fit: naturalW*scale == viewportW → maxX = 0
        act(() => api.zoomBy(4)) // 0.5 → 2.0
        expect(api.zoom.scale).toBe(2)
        act(() => api.panBy(10000, -10000))
        expect(api.zoom.offsetX).toBe(750) // (1000*2 - 500) / 2
        expect(api.zoom.offsetY).toBe(750)
        act(() => api.panBy(-10000, -10000))
        expect(api.zoom.offsetX).toBe(-750)
        expect(api.zoom.offsetY).toBe(-750)
        act(() => root.unmount())
      })

      it('toggleFit switches between fit and 100% and resets offset', () => {
        const { api, root } = mount()
        act(() => api.open(1000, 1000, 500, 500))
        act(() => api.zoomAt(2, 100, 50))
        act(() => api.toggleFit())
        expect(api.zoom.scale).toBe(api.zoom.fitScale)
        expect(api.zoom.offsetX).toBe(0)
        act(() => api.toggleFit())
        expect(api.zoom.scale).toBe(1)
        act(() => root.unmount())
      })

      it('reset returns to fit centered', () => {
        const { api, root } = mount()
        act(() => api.open(1000, 1000, 500, 500))
        act(() => api.zoomAt(2, 100, 50))
        act(() => api.panBy(10, 20))
        act(() => api.reset())
        expect(api.zoom.scale).toBe(api.zoom.fitScale)
        expect(api.zoom.offsetX).toBe(0)
        expect(api.zoom.offsetY).toBe(0)
        act(() => root.unmount())
      })

      it('derives displayPercent and isFit', () => {
        const { api, root } = mount()
        act(() => api.open(1000, 1000, 500, 500))
        expect(api.isFit).toBe(true)
        expect(api.displayPercent).toBe(50)
        act(() => api.zoomAt(2, 0, 0))
        expect(api.isFit).toBe(false)
        expect(api.displayPercent).toBe(100)
        act(() => api.zoomBy(1.25))
        expect(api.displayPercent).toBe(125)
        act(() => root.unmount())
      })
    })
  impl_code: |
    import { useCallback, useState } from 'react'

    export const MAX_ZOOM = 8
    export const WHEEL_STEP = 1.1
    export const BUTTON_STEP = 1.25

    export interface ZoomState {
      /** Scale relative to natural pixels: 1.0 = 100%. */
      scale: number
      /** Translation of the image center relative to the viewport center (px). */
      offsetX: number
      offsetY: number
      /** Contain-fit scale computed on open (never upscales small images). */
      fitScale: number
      viewportW: number
      viewportH: number
      naturalW: number
      naturalH: number
    }

    const EPS = 1e-9

    const clamp = (v: number, lo: number, hi: number): number =>
      Math.min(hi, Math.max(lo, v))

    function clampOffset(
      z: ZoomState,
      scale: number,
      offsetX: number,
      offsetY: number,
    ): { offsetX: number; offsetY: number } {
      const maxX = Math.max(0, (z.naturalW * scale - z.viewportW) / 2)
      const maxY = Math.max(0, (z.naturalH * scale - z.viewportH) / 2)
      return {
        offsetX: clamp(offsetX, -maxX, maxX),
        offsetY: clamp(offsetY, -maxY, maxY),
      }
    }

    export function useImageZoom() {
      const [zoom, setZoom] = useState<ZoomState>({
        scale: 1,
        offsetX: 0,
        offsetY: 0,
        fitScale: 1,
        viewportW: 0,
        viewportH: 0,
        naturalW: 0,
        naturalH: 0,
      })

      /** Initialize for a new image/viewport; resets to fit. */
      const open = useCallback(
        (naturalW: number, naturalH: number, viewportW: number, viewportH: number) => {
          const fitScale =
            naturalW > 0 && naturalH > 0 && viewportW > 0 && viewportH > 0
              ? Math.min(viewportW / naturalW, viewportH / naturalH, 1)
              : 1
          setZoom({
            scale: fitScale,
            offsetX: 0,
            offsetY: 0,
            fitScale,
            viewportW,
            viewportH,
            naturalW,
            naturalH,
          })
        },
        [],
      )

      /** Scale by `factor`, keeping the point under `anchor` (viewport coords, origin = viewport center) fixed. */
      const zoomAt = useCallback((factor: number, anchorX: number, anchorY: number) => {
        setZoom((z) => {
          const next = clamp(z.scale * factor, z.fitScale, MAX_ZOOM)
          const ratio = next / z.scale
          const rawX = anchorX - (anchorX - z.offsetX) * ratio
          const rawY = anchorY - (anchorY - z.offsetY) * ratio
          return { ...z, scale: next, ...clampOffset(z, next, rawX, rawY) }
        })
      }, [])

      /** Scale by `factor` anchored at the viewport center (buttons/keyboard). */
      const zoomBy = useCallback((factor: number) => {
        setZoom((z) => {
          const next = clamp(z.scale * factor, z.fitScale, MAX_ZOOM)
          const ratio = next / z.scale
          return { ...z, scale: next, ...clampOffset(z, next, z.offsetX * ratio, z.offsetY * ratio) }
        })
      }, [])

      /** Pan by viewport-space deltas; clamped so the image never leaves the viewport. */
      const panBy = useCallback((dx: number, dy: number) => {
        setZoom((z) => ({ ...z, ...clampOffset(z, z.scale, z.offsetX + dx, z.offsetY + dy) }))
      }, [])

      /** Toggle between fit and 100% (1:1 pixels). */
      const toggleFit = useCallback(() => {
        setZoom((z) => {
          if (Math.abs(z.scale - z.fitScale) < EPS) {
            const next = Math.min(1, MAX_ZOOM)
            return { ...z, scale: next, ...clampOffset(z, next, z.offsetX, z.offsetY) }
          }
          return { ...z, scale: z.fitScale, offsetX: 0, offsetY: 0 }
        })
      }, [])

      /** Reset to fit, centered. */
      const reset = useCallback(() => {
        setZoom((z) => ({ ...z, scale: z.fitScale, offsetX: 0, offsetY: 0 }))
      }, [])

      const isFit = Math.abs(zoom.scale - zoom.fitScale) < EPS
      const displayPercent = Math.round(zoom.scale * 100)

      return { zoom, open, zoomAt, zoomBy, panBy, toggleFit, reset, isFit, displayPercent }
    }
  verify:
    - "cd web-ui && npx vitest run src/hooks/useImageZoom.test.ts   # 9 tests PASS"
    - "cd web-ui && npm run build"
```

- [ ] **Step 1**: 将 `test_code` 写入 `web-ui/src/hooks/useImageZoom.test.ts`，运行 `cd web-ui && npx vitest run src/hooks/useImageZoom.test.ts` → 期望 FAIL（"Cannot find module './useImageZoom'"）
- [ ] **Step 2**: 将 `impl_code` 写入 `web-ui/src/hooks/useImageZoom.ts`，重跑 → PASS（9 tests）
- [ ] **Step 3**: `cd web-ui && npm run build` → 零错误
- [ ] **Step 4**: Commit

```bash
git add web-ui/src/hooks/useImageZoom.ts web-ui/src/hooks/useImageZoom.test.ts
git commit -m "feat: add useImageZoom hook with anchored zoom math and clamping"
```

---

### Task T1: Add zoom viewer i18n keys to en and zh-CN

**Files:**
- Modify: `web-ui/src/i18n/i18n.test.ts`（末尾第 151 行 `})` 之后追加存在性测试）
- Modify: `web-ui/src/i18n/locales/en.json`
- Modify: `web-ui/src/i18n/locales/zh-CN.json`

**Interfaces:**
- Produces: `imageDetail.zoomIn/zoomOut/zoomReset/zoomFit/zoomLevel/openViewer` 键（T2 的 ImageViewer 消费 `zoomIn/zoomOut/zoomFit/zoomLevel` + `modal.close`；`openViewer` 供 T3 预览卡片 aria-label；`zoomReset` 为保留键双写一致）。**必须先于 T2**：`npm run build`（tsc -b）以 en.json 为类型化 t() 键源，键不存在则 T2 编译失败。

```yaml
- id: T1
  title: "Add zoom viewer i18n keys to en and zh-CN"
  files:
    - Modify: web-ui/src/i18n/i18n.test.ts
    - Modify: web-ui/src/i18n/locales/en.json
    - Modify: web-ui/src/i18n/locales/zh-CN.json
  depends_on: []
  breaking: false
  ac:
    - given: "both locale catalogs contain the imageDetail block"
      when: "the 6 zoom keys (zoomIn/zoomOut/zoomReset/zoomFit/zoomLevel/openViewer) are looked up in each"
      then: "both en and zh-CN contain all 6 with non-empty values, and the key-set equality test in i18n.test.ts still passes"
  regression:
    - "cd web-ui && npx vitest run src/i18n/i18n.test.ts   # existing key-set equality test must keep passing"
  test_code: |
    // Append to web-ui/src/i18n/i18n.test.ts after line 151 (the final `})`):
    it('imageDetail zoom viewer keys exist in both locales', async () => {
      const en = (await import('./locales/en.json')).default as any
      const zh = (await import('./locales/zh-CN.json')).default as any
      for (const key of ['zoomIn', 'zoomOut', 'zoomReset', 'zoomFit', 'zoomLevel', 'openViewer']) {
        expect(en.imageDetail[key]).toBeTruthy()
        expect(zh.imageDetail[key]).toBeTruthy()
      }
    })
  impl_code: |
    // en.json — replace the last line of the imageDetail block:
    //   "notFound": "Image not found."
    // with:
    //   "notFound": "Image not found.",
    //   "zoomIn": "Zoom in",
    //   "zoomOut": "Zoom out",
    //   "zoomReset": "Reset zoom",
    //   "zoomFit": "Fit to screen",
    //   "zoomLevel": "Zoom {{percent}}%",
    //   "openViewer": "View image"

    // zh-CN.json — replace the last line of the imageDetail block:
    //   "notFound": "未找到图片。"
    // with:
    //   "notFound": "未找到图片。",
    //   "zoomIn": "放大",
    //   "zoomOut": "缩小",
    //   "zoomReset": "重置缩放",
    //   "zoomFit": "适应屏幕",
    //   "zoomLevel": "缩放 {{percent}}%",
    //   "openViewer": "查看图片"
  verify:
    - "cd web-ui && npx vitest run src/i18n/i18n.test.ts   # all PASS incl. key-set equality"
    - "cd web-ui && npm run build                          # typed t() keys compile"
```

- [ ] **Step 1**: 将 `test_code` 追加到 `web-ui/src/i18n/i18n.test.ts` 末尾，运行 `cd web-ui && npx vitest run src/i18n/i18n.test.ts` → 期望 FAIL（zoomIn 为 falsy）
- [ ] **Step 2**: 按 `impl_code` 修改 en.json / zh-CN.json（双写 6 键），重跑 → PASS
- [ ] **Step 3**: `cd web-ui && npm run build` → 零错误
- [ ] **Step 4**: Commit

```bash
git add web-ui/src/i18n/i18n.test.ts web-ui/src/i18n/locales/en.json web-ui/src/i18n/locales/zh-CN.json
git commit -m "feat(i18n): add zoom viewer keys to en and zh-CN"
```

---

### Task T2: Add fullscreen ImageViewer with zoom/pan/pinch gestures

**Files:**
- Create: `web-ui/src/components/ImageViewer.tsx`
- Create: `web-ui/src/components/ImageViewer.test.tsx`

**Interfaces:**
- Consumes: `useImageZoom` 全部导出（T0）；`useOverlay(onClose, enabled)`（现有）；`ui/Button` variant="icon"（现有）；i18n 键（T1）
- Produces: `<ImageViewer open: boolean, src: string, alt?: string, naturalWidth: number | null, naturalHeight: number | null, onClose: () => void />`；testid `viewer-overlay` / `viewer-surface` / `viewer-zoom-level` / `viewer-zoom-in` / `viewer-zoom-out` / `viewer-close`

```yaml
- id: T2
  title: "Add fullscreen ImageViewer with zoom/pan/pinch gestures"
  files:
    - Create: web-ui/src/components/ImageViewer.tsx
    - Create: web-ui/src/components/ImageViewer.test.tsx
  depends_on: [T0, T1]
  breaking: false
  ac:
    - given: "open=false"
      when: "ImageViewer is rendered"
      then: "no viewer-overlay element exists in the DOM"
    - given: "open=true with src + natural size"
      when: "rendered"
      then: "viewer-overlay and viewer-surface render, img has the src and draggable=false, body overflow is hidden, and close/zoom buttons have data-testid attributes"
    - given: "viewer open at fit (100%)"
      when: "a wheel event with deltaY -100 then +100 is dispatched on the surface"
      then: "viewer-zoom-level text contains 110% then 100%"
    - given: "viewer open at fit (100%)"
      when: "zoom-in, zoom-out, then zoom-in + percentage-button clicks are performed"
      then: "viewer-zoom-level shows 125%, 100%, 125%, then 100% (percentage click = reset to fit)"
    - given: "viewer open"
      when: "a pointer drag of +60/+30 px is performed on the surface"
      then: "the img transform contains translate(60px, 30px)"
    - given: "viewer open"
      when: "two pointers land 100px apart and the second moves to 200px"
      then: "viewer-zoom-level shows 200% (distance-ratio pinch)"
    - given: "viewer open"
      when: "double-click on the img after zooming out"
      then: "zoom toggles back to fit (100%); keyboard '0' resets to fit"
    - given: "viewer open"
      when: "Escape keydown is dispatched"
      then: "onClose is called exactly once"
    - given: "viewer open"
      when: "a tap (pointerdown+up, no movement) lands on the surface background"
      then: "onClose is called; the same tap on the img does not close"
  regression:
    - "cd web-ui && npx vitest run src/components/ui/Modal.test.tsx src/hooks/useOverlay.test.tsx   # overlay primitives ImageViewer builds on"
  test_code: |
    import { describe, it, expect, vi, beforeEach } from 'vitest'
    import { act } from 'react'
    import { createRoot, type Root } from 'react-dom/client'
    import ImageViewer from './ImageViewer'

    function render(node: React.ReactNode): Root {
      const container = document.createElement('div')
      document.body.appendChild(container)
      const root = createRoot(container)
      act(() => root.render(node))
      return root
    }

    const PROPS = { src: 'http://localhost/u/abc', naturalWidth: 1000, naturalHeight: 1000 }

    const overlay = () => document.querySelector('[data-testid="viewer-overlay"]')
    const surface = () => document.querySelector('[data-testid="viewer-surface"]')!
    const level = () => document.querySelector('[data-testid="viewer-zoom-level"]')!

    describe('ImageViewer', () => {
      beforeEach(() => {
        document.body.style.overflow = ''
      })

      it('renders nothing when closed', () => {
        const root = render(<ImageViewer open={false} {...PROPS} onClose={vi.fn()} />)
        expect(overlay()).toBeNull()
        act(() => root.unmount())
      })

      it('renders overlay with image and toolbar when open; Escape closes', () => {
        const onClose = vi.fn()
        const root = render(<ImageViewer open {...PROPS} onClose={onClose} />)
        expect(overlay()).toBeTruthy()
        expect(surface()).toBeTruthy()
        expect(document.body.style.overflow).toBe('hidden')
        expect(surface().querySelector('img')!.getAttribute('src')).toBe(PROPS.src)
        expect(surface().querySelector('img')!.getAttribute('draggable')).toBe('false')
        expect(document.querySelector('[data-testid="viewer-zoom-in"]')).toBeTruthy()
        expect(document.querySelector('[data-testid="viewer-zoom-out"]')).toBeTruthy()
        expect(document.querySelector('[data-testid="viewer-close"]')).toBeTruthy()
        act(() => document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' })))
        expect(onClose).toHaveBeenCalledTimes(1)
        act(() => root.unmount())
      })

      it('wheel zooms in on negative deltaY and out on positive', () => {
        const root = render(<ImageViewer open {...PROPS} onClose={vi.fn()} />)
        const el = surface()
        act(() => el.dispatchEvent(new WheelEvent('wheel', { deltaY: -100, bubbles: true })))
        expect(level()!.textContent).toContain('110%')
        act(() => el.dispatchEvent(new WheelEvent('wheel', { deltaY: 100, bubbles: true })))
        expect(level()!.textContent).toContain('100%')
        act(() => root.unmount())
      })

      it('toolbar buttons zoom in/out and percentage resets to fit', () => {
        const root = render(<ImageViewer open {...PROPS} onClose={vi.fn()} />)
        const zoomIn = document.querySelector('[data-testid="viewer-zoom-in"]')!
        const zoomOut = document.querySelector('[data-testid="viewer-zoom-out"]')!
        expect(zoomIn).toBeTruthy()
        expect(zoomOut).toBeTruthy()
        act(() => zoomIn.click())
        expect(level()!.textContent).toContain('125%')
        act(() => zoomOut.click())
        expect(level()!.textContent).toContain('100%')
        act(() => zoomIn.click())
        act(() => level()!.click()) // percentage button = reset to fit
        expect(level()!.textContent).toContain('100%')
        act(() => root.unmount())
      })

      it('drag pans the image and updates the img transform', () => {
        const root = render(<ImageViewer open {...PROPS} onClose={vi.fn()} />)
        const el = surface()
        const img = el.querySelector('img')!
        const fire = (type: string, x: number, y: number) =>
          el.dispatchEvent(new MouseEvent(type, { bubbles: true, clientX: x, clientY: y }))
        act(() => { fire('pointerdown', 200, 200); fire('pointermove', 260, 230); fire('pointerup', 260, 230) })
        expect(img.style.transform).toContain('translate(60px, 30px)')
        act(() => root.unmount())
      })

      it('pinch with two pointers zooms using distance ratio', () => {
        const root = render(<ImageViewer open {...PROPS} onClose={vi.fn()} />)
        const el = surface()
        const fire = (type: string, id: number, x: number, y: number) => {
          const ev = new MouseEvent(type, { bubbles: true, clientX: x, clientY: y })
          Object.defineProperty(ev, 'pointerId', { value: id })
          return ev
        }
        act(() => {
          el.dispatchEvent(fire('pointerdown', 1, 100, 100))
          el.dispatchEvent(fire('pointerdown', 2, 200, 100)) // dist 100
        })
        act(() => el.dispatchEvent(fire('pointermove', 2, 300, 100))) // dist 200 → ×2
        expect(level()!.textContent).toContain('200%')
        act(() => {
          el.dispatchEvent(fire('pointerup', 1, 100, 100))
          el.dispatchEvent(fire('pointerup', 2, 300, 100))
        })
        act(() => root.unmount())
      })

      it('double-click toggles fit <-> zoomed, keyboard 0 resets', () => {
        const root = render(<ImageViewer open {...PROPS} onClose={vi.fn()} />)
        const el = surface()
        const img = el.querySelector('img')!
        act(() => el.dispatchEvent(new WheelEvent('wheel', { deltaY: 100, bubbles: true })))
        expect(level()!.textContent).toContain('91%') // 1 / 1.1
        act(() => img.dispatchEvent(new MouseEvent('dblclick', { bubbles: true })))
        expect(level()!.textContent).toContain('100%') // back to fit
        act(() => el.dispatchEvent(new WheelEvent('wheel', { deltaY: -100, bubbles: true })))
        expect(level()!.textContent).toContain('110%')
        act(() => document.dispatchEvent(new KeyboardEvent('keydown', { key: '0' })))
        expect(level()!.textContent).toContain('100%')
        act(() => root.unmount())
      })

      it('tap without drag on the background closes the viewer', () => {
        const onClose = vi.fn()
        const root = render(<ImageViewer open {...PROPS} onClose={onClose} />)
        const el = surface()
        const fire = (type: string, x: number, y: number) =>
          el.dispatchEvent(new MouseEvent(type, { bubbles: true, clientX: x, clientY: y }))
        act(() => { fire('pointerdown', 10, 10); fire('pointerup', 10, 10) })
        expect(onClose).toHaveBeenCalledTimes(1)
        act(() => root.unmount())
      })

      it('tap without drag on the image does not close', () => {
        const onClose = vi.fn()
        const root = render(<ImageViewer open {...PROPS} onClose={onClose} />)
        const el = surface()
        const img = el.querySelector('img')!
        const fire = (type: string, x: number, y: number) =>
          img.dispatchEvent(new MouseEvent(type, { bubbles: true, clientX: x, clientY: y }))
        act(() => { fire('pointerdown', 10, 10); fire('pointerup', 10, 10) })
        expect(onClose).not.toHaveBeenCalled()
        act(() => root.unmount())
      })
    })
  impl_code: |
    import { useEffect, useRef, useState } from 'react'
    import { ZoomIn, ZoomOut, X } from 'lucide-react'
    import { useTranslation } from 'react-i18next'
    import useOverlay from '../hooks/useOverlay'
    import { useImageZoom, BUTTON_STEP, WHEEL_STEP } from '../hooks/useImageZoom'
    import Button from './ui/Button'

    interface ImageViewerProps {
      open: boolean
      src: string
      alt?: string
      naturalWidth: number | null
      naturalHeight: number | null
      onClose: () => void
    }

    /** Pointer movement beyond this (px) before a drag counts as pan. */
    const MOVE_THRESHOLD = 3

    export default function ImageViewer({
      open,
      src,
      alt,
      naturalWidth,
      naturalHeight,
      onClose,
    }: ImageViewerProps) {
      const { t } = useTranslation()
      const surfaceRef = useRef<HTMLDivElement>(null)
      const pointers = useRef(new Map<number, { x: number; y: number }>())
      const pinchDist = useRef(0)
      const dragging = useRef(false)
      const tapStart = useRef(false)
      const [natural, setNatural] = useState({ w: naturalWidth ?? 0, h: naturalHeight ?? 0 })
      const { zoom, open: openZoom, zoomAt, zoomBy, panBy, toggleFit, reset, displayPercent } =
        useImageZoom()
      const { overlayProps } = useOverlay(onClose, open)

      // (Re)initialize fit when opened or when natural size arrives.
      useEffect(() => {
        if (!open) return
        const el = surfaceRef.current
        if (!el) return
        openZoom(natural.w, natural.h, el.clientWidth, el.clientHeight)
      }, [open, natural, openZoom])

      // Keyboard shortcuts: + / - / 0.
      useEffect(() => {
        if (!open) return
        function handleKeyDown(e: KeyboardEvent) {
          if (e.key === '+' || e.key === '=') {
            e.preventDefault()
            zoomBy(BUTTON_STEP)
          } else if (e.key === '-') {
            e.preventDefault()
            zoomBy(1 / BUTTON_STEP)
          } else if (e.key === '0') {
            e.preventDefault()
            reset()
          }
        }
        document.addEventListener('keydown', handleKeyDown)
        return () => document.removeEventListener('keydown', handleKeyDown)
      }, [open, zoomBy, reset])

      // Wheel zoom — native listener with { passive: false } (React attaches wheel passively).
      useEffect(() => {
        if (!open) return
        const el = surfaceRef.current
        if (!el) return
        function handleWheel(e: WheelEvent) {
          e.preventDefault()
          const rect = el.getBoundingClientRect()
          const anchorX = e.clientX - rect.left - rect.width / 2
          const anchorY = e.clientY - rect.top - rect.height / 2
          zoomAt(e.deltaY < 0 ? WHEEL_STEP : 1 / WHEEL_STEP, anchorX, anchorY)
        }
        el.addEventListener('wheel', handleWheel, { passive: false })
        return () => el.removeEventListener('wheel', handleWheel)
      }, [open, zoomAt])

      if (!open) return null

      return (
        <div
          data-testid="viewer-overlay"
          {...overlayProps}
          className="fixed inset-0 z-[9999]"
          style={{ background: 'rgba(0, 0, 0, 0.9)' }}
        >
          <div
            ref={surfaceRef}
            data-testid="viewer-surface"
            className="absolute inset-0 overflow-hidden"
            style={{ touchAction: 'none', cursor: dragging.current ? 'grabbing' : 'grab' }}
            onPointerDown={(e) => {
              if (e.target === e.currentTarget) tapStart.current = true
              pointers.current.set(e.pointerId ?? 1, { x: e.clientX, y: e.clientY })
              try {
                e.currentTarget.setPointerCapture(e.pointerId ?? 1)
              } catch {
                /* jsdom / unsupported — capture is best-effort */
              }
            }}
            onPointerMove={(e) => {
              const id = e.pointerId ?? 1
              const prev = pointers.current.get(id)
              if (!prev) return
              const cur = { x: e.clientX, y: e.clientY }
              pointers.current.set(id, cur)
              if (pointers.current.size === 2) {
                const [a, b] = [...pointers.current.values()]
                const dist = Math.hypot(a.x - b.x, a.y - b.y)
                if (pinchDist.current > 0) {
                  const el = surfaceRef.current
                  if (el) {
                    const rect = el.getBoundingClientRect()
                    const midX = (a.x + b.x) / 2 - rect.left - rect.width / 2
                    const midY = (a.y + b.y) / 2 - rect.top - rect.height / 2
                    zoomAt(dist / pinchDist.current, midX, midY)
                  }
                }
                pinchDist.current = dist
              } else {
                if (Math.hypot(cur.x - prev.x, cur.y - prev.y) > MOVE_THRESHOLD) {
                  dragging.current = true
                  tapStart.current = false
                }
                if (dragging.current) panBy(cur.x - prev.x, cur.y - prev.y)
              }
            }}
            onPointerUp={(e) => {
              const id = e.pointerId ?? 1
              pointers.current.delete(id)
              if (pointers.current.size < 2) pinchDist.current = 0
              if (pointers.current.size === 0) {
                if (tapStart.current && e.target === e.currentTarget) onClose()
                tapStart.current = false
                dragging.current = false
              }
            }}
            onPointerCancel={(e) => {
              pointers.current.delete(e.pointerId ?? 1)
              if (pointers.current.size < 2) pinchDist.current = 0
              if (pointers.current.size === 0) {
                tapStart.current = false
                dragging.current = false
              }
            }}
            onDoubleClick={toggleFit}
          >
            <img
              src={src}
              alt={alt}
              draggable={false}
              onLoad={(e) => {
                if (natural.w === 0 || natural.h === 0) {
                  setNatural({ w: e.currentTarget.naturalWidth, h: e.currentTarget.naturalHeight })
                }
              }}
              className="select-none will-change-transform"
              style={{
                transform: `translate(${zoom.offsetX}px, ${zoom.offsetY}px) scale(${zoom.scale})`,
                transformOrigin: 'center',
                maxWidth: 'none',
                maxHeight: 'none',
              }}
            />
          </div>

          <div
            className="pointer-events-none absolute inset-x-0 bottom-0 flex justify-center"
            style={{ paddingBottom: 'max(env(safe-area-inset-bottom), 1rem)' }}
          >
            <div
              className="pointer-events-auto flex items-center gap-1 rounded-full px-2 py-1"
              style={{
                background: 'var(--glass-bg)',
                border: '1px solid var(--color-border)',
                backdropFilter: 'blur(8px)',
              }}
            >
              <Button
                variant="icon"
                size="md"
                data-testid="viewer-zoom-out"
                aria-label={t('imageDetail.zoomOut')}
                onClick={() => zoomBy(1 / BUTTON_STEP)}
              >
                <ZoomOut className="h-5 w-5" />
              </Button>
              <button
                data-testid="viewer-zoom-level"
                aria-label={t('imageDetail.zoomFit')}
                title={t('imageDetail.zoomFit')}
                onClick={reset}
                className="min-w-14 px-2 text-sm font-medium"
                style={{ color: 'var(--color-text-secondary)' }}
              >
                {t('imageDetail.zoomLevel', { percent: displayPercent })}
              </button>
              <Button
                variant="icon"
                size="md"
                data-testid="viewer-zoom-in"
                aria-label={t('imageDetail.zoomIn')}
                onClick={() => zoomBy(BUTTON_STEP)}
              >
                <ZoomIn className="h-5 w-5" />
              </Button>
              <div className="mx-1 h-5 w-px" style={{ background: 'var(--color-border)' }} />
              <Button
                variant="icon"
                size="md"
                data-testid="viewer-close"
                aria-label={t('modal.close')}
                onClick={onClose}
              >
                <X className="h-5 w-5" />
              </Button>
            </div>
          </div>
        </div>
      )
    }
  verify:
    - "cd web-ui && npx vitest run src/components/ImageViewer.test.tsx src/hooks/useImageZoom.test.ts   # 9 + 9 PASS"
    - "cd web-ui && npm run build"
```

- [ ] **Step 1**: 将 `test_code` 写入 `web-ui/src/components/ImageViewer.test.tsx`，运行 → 期望 FAIL（"Cannot find module './ImageViewer'"）
- [ ] **Step 2**: 将 `impl_code` 写入 `web-ui/src/components/ImageViewer.tsx`，重跑 → PASS（9 tests）
- [ ] **Step 3**: `cd web-ui && npm run build` → 零错误（类型化 t() 键此时已存在，T1 前置）
- [ ] **Step 4**: Commit

```bash
git add web-ui/src/components/ImageViewer.tsx web-ui/src/components/ImageViewer.test.tsx
git commit -m "feat: add fullscreen ImageViewer with zoom/pan/pinch gestures"
```

---

### Task T3: Integrate zoom viewer into ImageDetail with E2E coverage

**Files:**
- Modify: `web-ui/src/pages/ImageDetail.tsx`
- Modify: `web-ui/e2e/page-objects/image-detail.po.ts`
- Test: `web-ui/e2e/specs/image-detail.spec.ts`

**Interfaces:**
- Consumes: `ImageViewer`（T2）、i18n 键（T1）、spec 全局 `imageId`/`auth`/`FIXTURES.png200`（现有）
- Produces: 预览卡片 `data-testid="image-preview"`；page-object getters `imagePreview/viewerOverlay/viewerSurface/viewerZoomLevel/viewerZoomIn/viewerZoomOut/viewerClose`

```yaml
- id: T3
  title: "Integrate zoom viewer into ImageDetail with E2E coverage"
  files:
    - Modify: web-ui/src/pages/ImageDetail.tsx
    - Modify: web-ui/e2e/page-objects/image-detail.po.ts
    - Modify: web-ui/e2e/specs/image-detail.spec.ts
  depends_on: [T2]
  breaking: false
  ac:
    - given: "an uploaded 200×200 image on the detail page (desktop viewport)"
      when: "clicking the image-preview card"
      then: "viewer-overlay becomes visible and viewer-zoom-level reads 'Zoom 100%'"
    - given: "viewer open"
      when: "mouse wheel (0, -100) is dispatched over the surface center"
      then: "viewer-zoom-level reads 'Zoom 110%'"
    - given: "viewer open at 110%"
      when: "zoom-in button clicked, then zoom-out, then keyboard '0'"
      then: "viewer-zoom-level reads 'Zoom 138%', then 'Zoom 110%', then 'Zoom 100%'"
    - given: "viewer open"
      when: "Escape is pressed"
      then: "viewer-overlay is hidden"
    - given: "mobile 375×667 touch viewport"
      when: "tapping image-preview and clicking zoom-in"
      then: "viewer-overlay is visible, viewer-zoom-level reads 'Zoom 125%', and clicking viewer-close hides the overlay"
  regression:
    - "cd web-ui && npx vitest run                                    # all unit tests keep passing"
    - "cd web-ui && npx playwright test e2e/specs/image-detail.spec.ts  # existing 7 desktop + 1 mobile tests unaffected"
    - "cd web-ui && npm run build"
  test_code: |
    // ── e2e/specs/image-detail.spec.ts ──
    // 1) Insert INSIDE test.describe.serial('image-detail', ...), BEFORE the
    //    'public serving works via public key' test:
    test('zoom viewer: open, wheel/buttons zoom, reset, close on Escape', async ({ page, request }) => {
      await seedUserSession(page, request)
      const detail = new ImageDetailPage(page)
      await detail.goto(imageId)
      await detail.imagePreview.click()
      await expect(detail.viewerOverlay).toBeVisible()
      await expect(detail.viewerZoomLevel).toHaveText('Zoom 100%')

      // Wheel zoom in (×1.1 per notch) — hover the surface center first
      const box = await detail.viewerSurface.boundingBox()
      if (!box) throw new Error('viewer surface has no bounding box')
      await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2)
      await page.mouse.wheel(0, -100)
      await expect(detail.viewerZoomLevel).toHaveText('Zoom 110%')

      // Toolbar zoom in (×1.25) → 137.5 → 138%; zoom out returns
      await detail.viewerZoomIn.click()
      await expect(detail.viewerZoomLevel).toHaveText('Zoom 138%')
      await detail.viewerZoomOut.click()
      await expect(detail.viewerZoomLevel).toHaveText('Zoom 110%')

      // Keyboard 0 resets to fit
      await page.keyboard.press('0')
      await expect(detail.viewerZoomLevel).toHaveText('Zoom 100%')

      // Escape closes
      await page.keyboard.press('Escape')
      await expect(detail.viewerOverlay).toBeHidden()
    })

    // 2) Append INSIDE test.describe('image detail on mobile', ...) after the
    //    'rename pencil visible without hover on touch' test:
    test('zoom viewer toolbar works on touch', async ({ page, request }) => {
      await seedUserSession(page, request)
      const detail = new ImageDetailPage(page)
      await detail.goto(imageId)
      await detail.imagePreview.tap()
      await expect(detail.viewerOverlay).toBeVisible()
      await expect(detail.viewerZoomLevel).toHaveText('Zoom 100%')
      await detail.viewerZoomIn.click()
      await expect(detail.viewerZoomLevel).toHaveText('Zoom 125%')
      await detail.viewerClose.click()
      await expect(detail.viewerOverlay).toBeHidden()
    })

    // ── e2e/page-objects/image-detail.po.ts ──
    // Insert BEFORE the goto method:
    //   /** Image preview card — clicking opens the zoom viewer. */
    //   get imagePreview() { return this.page.getByTestId('image-preview') }
    //   /** Fullscreen zoom viewer overlay. */
    //   get viewerOverlay() { return this.page.getByTestId('viewer-overlay') }
    //   /** The interactive zoom surface (gesture area). */
    //   get viewerSurface() { return this.page.getByTestId('viewer-surface') }
    //   /** Zoom percentage label button (click = reset to fit). */
    //   get viewerZoomLevel() { return this.page.getByTestId('viewer-zoom-level') }
    //   get viewerZoomIn() { return this.page.getByTestId('viewer-zoom-in') }
    //   get viewerZoomOut() { return this.page.getByTestId('viewer-zoom-out') }
    //   get viewerClose() { return this.page.getByTestId('viewer-close') }
  impl_code: |
    // ── web-ui/src/pages/ImageDetail.tsx ──
    // 1) After line 17 (`import { useFormat } from '../hooks/useFormat'`) add:
    import ImageViewer from '../components/ImageViewer'

    // 2) After line 64 (`const [linkFormat, setLinkFormat] = useState<LinkFormat>('url')`) add:
      const [viewerOpen, setViewerOpen] = useState(false)

    // 3) Replace the image preview block (lines 163-170):
    //      {/* Image preview */}
    //      <div className="glass-elevated mb-4 overflow-hidden rounded-xl">
    //        <img src={img.url} alt={img.original_name} className="max-h-[60vh] w-full object-contain" />
    //      </div>
    //    with:
    //      {/* Image preview */}
    //      <div
    //        data-testid="image-preview"
    //        onClick={() => setViewerOpen(true)}
    //        className="glass-elevated mb-4 cursor-zoom-in overflow-hidden rounded-xl"
    //      >
    //        <img
    //          src={img.url}
    //          alt={img.original_name}
    //          className="max-h-[60vh] w-full object-contain"
    //        />
    //      </div>

    // 4) Before the page root div closing tag (line 345 `</div>`) add:
    //      <ImageViewer
    //        open={viewerOpen}
    //        src={img.url}
    //        alt={img.original_name}
    //        naturalWidth={img.width}
    //        naturalHeight={img.height}
    //        onClose={() => setViewerOpen(false)}
    //      />
  verify:
    - "cd web-ui && npx playwright test e2e/specs/image-detail.spec.ts   # 8 desktop + 2 mobile PASS (Docker PG+Redis, webServer auto-starts)"
    - "cd web-ui && npx vitest run"
    - "cd web-ui && npm run build"
```

- [ ] **Step 1**: 将 `test_code` 的 E2E 测试与 page-object getters 写入对应文件，运行 `cd web-ui && npx playwright test e2e/specs/image-detail.spec.ts` → 期望新测试 FAIL（`image-preview` testid 不存在）
- [ ] **Step 2**: 按 `impl_code` 修改 ImageDetail.tsx（4 处），重跑 E2E → PASS（8 desktop + 2 mobile）
- [ ] **Step 3**: `cd web-ui && npx vitest run` + `npm run build` → 全绿
- [ ] **Step 4**: Commit

```bash
git add web-ui/src/pages/ImageDetail.tsx web-ui/e2e/page-objects/image-detail.po.ts web-ui/e2e/specs/image-detail.spec.ts
git commit -m "feat: integrate zoom viewer into image detail page with E2E coverage"
```

---

### Task T4: Bump version to 0.20.0

**Files:**
- Modify: `Cargo.toml`（根，第 6 行）
- Modify: `web-ui/package.json`
- Modify: `CHANGELOG.md`（第 9 行之前插入）

**Interfaces:**
- 无（meta 任务）。前置：T0-T3 全部合并

```yaml
- id: T4
  title: "Bump version to 0.20.0"
  files:
    - Modify: Cargo.toml
    - Modify: web-ui/package.json
    - Modify: CHANGELOG.md
  depends_on: [T3]
  breaking: false
  ac:
    - given: "current version is 0.19.1 in Cargo.toml and package.json"
      when: "the bump is applied"
      then: "Cargo.toml line 6 reports version = \"0.20.0\", package.json reports \"version\": \"0.20.0\", and CHANGELOG.md gains a [0.20.0] - 2026-08-09 section above [0.19.1]"
  regression:
    - "cd web-ui && npm run build"
  test_code: |
    # Failing pre-condition check (must print FAIL before the bump):
    grep -q '^version = "0.20.0"' Cargo.toml && grep -q '"version": "0.20.0"' web-ui/package.json && echo PASS || echo FAIL
  impl_code: |
    # Cargo.toml line 6:  version = "0.19.1"  →  version = "0.20.0"
    # web-ui/package.json:  "version": "0.19.1"  →  "version": "0.20.0"
    # CHANGELOG.md — insert above "## [0.19.1] - 2026-08-09":
    ## [0.20.0] - 2026-08-09

    ### Added

    - Image detail zoom viewer: click the image to open a fullscreen lightbox with cursor-anchored wheel zoom, drag pan, double-click fit↔100% toggle, two-finger pinch / single-finger drag on touch devices, toolbar zoom in/out/reset buttons with percentage display, and keyboard `+`/`-`/`0` shortcuts.
  verify:
    - "grep -q '^version = \"0.20.0\"' Cargo.toml && grep -q '\"version\": \"0.20.0\"' web-ui/package.json"
    - "cd web-ui && npm run build"
```

- [ ] **Step 1**: 运行 `test_code` 的 grep → 期望 FAIL
- [ ] **Step 2**: 按 `impl_code` 执行三处修改
- [ ] **Step 3**: 重跑 grep → PASS；`cd web-ui && npm run build` → 零错误
- [ ] **Step 4**: Commit

```bash
git add Cargo.toml web-ui/package.json CHANGELOG.md
git commit -m "chore: bump version to 0.20.0"
```

---

### Task T5: Sync docs (AGENTS.md / README.md / summary)

**Files:**
- Modify: `AGENTS.md`
- Modify: `README.md`
- Modify: `.omo/summary/summary_and_next.md`

**Interfaces:**
- 无（meta 任务）。前置：T4

```yaml
- id: T5
  title: "Sync docs (AGENTS.md / README.md / summary)"
  files:
    - Modify: AGENTS.md
    - Modify: README.md
    - Modify: .omo/summary/summary_and_next.md
  depends_on: [T4]
  breaking: false
  ac:
    - given: "the feature is merged"
      when: "the three docs are updated"
      then: "AGENTS.md version line reads 0.20.0 and lists ImageViewer/useImageZoom; README.md tagline and Features list mention the zoom viewer; summary_and_next.md gains a completed-phase section — verified by git diff --stat showing all three files changed"
  regression:
    - "cd web-ui && npm run build"
  test_code: |
    # Failing pre-condition check (must print the docs unchanged before sync):
    git diff --stat HEAD~1 -- AGENTS.md README.md .omo/summary/summary_and_next.md | grep -q . && echo CHANGED || echo UNCHANGED
  impl_code: |
    # AGENTS.md:
    #   - Version line:  - Version: `0.20.0` — image detail zoom viewer (fullscreen lightbox:
    #     wheel/drag/pinch zoom, toolbar + keyboard controls) + responsive layout + i18n complete (...)
    #   - Components list: add `ImageViewer` (fullscreen zoom viewer — click the image to open;
    #     wheel/button/keyboard/pinch zoom, drag pan, double-click fit↔100%)
    #   - Hooks list: add `useImageZoom` (pure zoom/pan state — scale/offset, anchored zoom math, range/pan clamping)
    # README.md:
    #   - Tagline:  **v0.20.0** — image detail zoom viewer. Fullscreen lightbox with cursor-anchored
    #     wheel zoom, drag pan, pinch gestures, toolbar + keyboard controls. (keep responsive/i18n summary)
    #   - Features list: add `- [x] **Image detail zoom viewer** — fullscreen lightbox: wheel zoom
    #     (cursor-anchored), drag pan, double-click fit↔100%, touch pinch/drag, toolbar zoom controls, keyboard shortcuts`
    #   - Project Structure: add `ImageViewer (zoom viewer)` under components/, `useImageZoom` under hooks/
    # .omo/summary/summary_and_next.md:
    #   - Add top section `## 图片详情页缩放查看器 ✅ (本次完成)`: useImageZoom (anchored math/clamping, 9 TDD tests),
    #     ImageViewer (useOverlay overlay + Pointer Events gestures + glass toolbar, 9 tests), i18n 6 keys,
    #     2 new E2E tests, version 0.19.1 → 0.20.0; verification: npx vitest run / npm run build /
    #     npx playwright test e2e/specs/image-detail.spec.ts
  verify:
    - "git diff --stat HEAD~1 -- AGENTS.md README.md .omo/summary/summary_and_next.md"
    - "cd web-ui && npm run build"
```

- [ ] **Step 1**: 运行 `test_code` 的 git diff → 期望 UNCHANGED
- [ ] **Step 2**: 按 `impl_code` 更新三个文档
- [ ] **Step 3**: 重跑 git diff → CHANGED；`cd web-ui && npm run build` → 零错误
- [ ] **Step 4**: Commit

```bash
git add AGENTS.md README.md .omo/summary/summary_and_next.md
git commit -m "docs: auto-sync AGENTS.md, README.md, summary after zoom viewer completion"
```

---

## Self-Review（writing-plans skill）

1. **Spec coverage**: spec §3 组件（T0/T2）、§4 状态模型（T0）、§5 交互（T0 数学 + T2 事件）、§6 UI（T2）、§7 i18n（T1）、§8 测试（T0/T2 vitest + T3 E2E）、§9 版本（T4）、文档同步（T5）— 全覆盖。
2. **Placeholder scan**: 所有 test_code/impl_code 均为完整可运行代码/精确 diff 说明，无 TBD/"similar to task N"。
3. **Type consistency**: `useImageZoom` 签名 T0/T2 一致；`ImageViewer` props T2/T3 一致（`naturalWidth/Height: number | null` ↔ `ImageInfo.width/height`）；testid `viewer-overlay/surface/zoom-level/zoom-in/zoom-out/close` 在 T2/T3 一致；i18n 键名 T1/T2/T3 一致。
4. **Task ordering**: T1（i18n）先于 T2（ImageViewer）— `npm run build` 的 tsc -b 类型化键依赖已消除；depends_on 图无环。
