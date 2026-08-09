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
