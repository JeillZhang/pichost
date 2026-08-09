import { useEffect } from 'react'

/**
 * Shared overlay behavior for Modal / Sheet / MobileNav:
 * Escape closes, body scroll locks while open, overlay click closes.
 * Panel clicks must stopPropagation to avoid closing.
 *
 * `enabled` gates the scroll lock + Escape listener so closed overlays
 * never freeze body scroll. Pass the overlay's `open` state.
 */
export default function useOverlay(onClose: () => void, enabled: boolean) {
  useEffect(() => {
    if (!enabled) return
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
  }, [onClose, enabled])

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
