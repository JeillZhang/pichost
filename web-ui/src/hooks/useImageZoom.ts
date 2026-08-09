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
