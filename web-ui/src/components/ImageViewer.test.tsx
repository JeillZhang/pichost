import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import '../i18n' // init i18next instance for useTranslation (component under test)
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
const level = () => document.querySelector('[data-testid="viewer-zoom-level"]')! as HTMLElement

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
    // surface must center the image so the zoom hook's center-origin math holds
    expect(surface().className).toContain('flex items-center justify-center')
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
    const zoomIn = document.querySelector('[data-testid="viewer-zoom-in"]')! as HTMLElement
    const zoomOut = document.querySelector('[data-testid="viewer-zoom-out"]')! as HTMLElement
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
    // Wheel-out below fit is clamped at fitScale (=1 in jsdom, per useImageZoom),
    // so zoom IN first to get a state the double-click can toggle back from.
    act(() => el.dispatchEvent(new WheelEvent('wheel', { deltaY: -100, bubbles: true })))
    expect(level()!.textContent).toContain('110%')
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
