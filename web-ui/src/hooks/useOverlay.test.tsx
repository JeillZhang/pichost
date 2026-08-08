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
