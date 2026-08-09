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
