import { describe, it, expect, vi, beforeEach } from 'vitest'
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import useOverlay from './useOverlay'

// Test harness: a tiny component consuming the hook.
// Children render INSIDE the overlay so clicks on them bubble to it.
function Harness({
  onClose,
  enabled = true,
  children,
}: {
  onClose: () => void
  enabled?: boolean
  children?: React.ReactNode
}) {
  const { overlayProps } = useOverlay(onClose, enabled)
  return (
    <div data-testid="overlay" {...overlayProps}>
      {children}
    </div>
  )
}

function renderHarness(onClose: () => void, children?: React.ReactNode): Root {
  const container = document.createElement('div')
  document.body.appendChild(container)
  const root = createRoot(container)
  act(() => root.render(<Harness onClose={onClose}>{children}</Harness>))
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

  it('does NOT close when a click inside the panel bubbles to the overlay (guard: target !== currentTarget)', () => {
    const onClose = vi.fn()
    const root = renderHarness(
      onClose,
      <div data-testid="panel">
        panel content
      </div>,
    )
    act(() => {
      document.querySelector('[data-testid="panel"]')!.dispatchEvent(
        new MouseEvent('mousedown', { bubbles: true }),
      )
    })
    // No stopPropagation on the panel: the mousedown reaches the overlay,
    // whose handler sees e.target (panel) !== e.currentTarget (overlay) → no close.
    expect(onClose).not.toHaveBeenCalled()
    act(() => root.unmount())
  })

  it('does not lock scroll or listen to Escape when disabled', () => {
    const onClose = vi.fn()
    const container = document.createElement('div')
    document.body.appendChild(container)
    const root = createRoot(container)
    act(() => root.render(<Harness onClose={onClose} enabled={false} />))
    expect(document.body.style.overflow).toBe('')
    act(() => document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape' })))
    expect(onClose).not.toHaveBeenCalled()
    act(() => root.unmount())
  })
})
