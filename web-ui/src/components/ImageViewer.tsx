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
      // Narrowing from the outer `if (!el) return` guard does not flow into
      // this closure (TS limitation) — el is non-null while the listener is attached.
      const rect = el!.getBoundingClientRect()
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
          // Establish the pinch baseline when the second finger lands, so the
          // first two-pointer move already zooms by the distance ratio.
          if (pointers.current.size === 2) {
            const [a, b] = [...pointers.current.values()]
            pinchDist.current = Math.hypot(a.x - b.x, a.y - b.y)
          }
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
