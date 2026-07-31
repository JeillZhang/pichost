import { type ReactNode, useCallback, useEffect, useRef, useState } from 'react'

export interface DropdownMenuItem {
  label: string
  icon?: ReactNode
  onClick: () => void
  danger?: boolean
}

interface DropdownMenuProps {
  trigger: ReactNode
  items: DropdownMenuItem[]
  align?: 'left' | 'right'
}

export default function DropdownMenu({
  trigger,
  items,
  align = 'right',
}: DropdownMenuProps) {
  const [open, setOpen] = useState(false)
  const containerRef = useRef<HTMLDivElement>(null)

  const close = useCallback(() => setOpen(false), [])

  useEffect(() => {
    if (!open) return
    function handlePointerDown(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        close()
      }
    }
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') close()
    }
    document.addEventListener('mousedown', handlePointerDown)
    document.addEventListener('keydown', handleKeyDown)
    return () => {
      document.removeEventListener('mousedown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [open, close])

  return (
    <div ref={containerRef} className="relative inline-flex">
      <div className="inline-flex items-center" onClick={() => setOpen((prev) => !prev)}>
        {trigger}
      </div>
      {open && (
        <div
          role="menu"
          className="absolute z-50 min-w-[120px] overflow-hidden rounded-lg py-1"
          style={{
            top: 'calc(100% + 0.375rem)',
            ...(align === 'right' ? { right: 0 } : { left: 0 }),
            backgroundColor: 'var(--color-surface-elevated)',
            border: '1px solid var(--color-border)',
            boxShadow: 'var(--glass-shadow)',
            backdropFilter: 'blur(var(--glass-blur))',
          }}
        >
          {items.map((item, index) => (
            <button
              key={index}
              type="button"
              role="menuitem"
              className="flex w-full items-center gap-2 px-3 py-1.5 text-left text-sm"
              style={{
                color: item.danger ? 'var(--color-danger)' : 'var(--color-text-secondary)',
                backgroundColor: 'transparent',
                transition: 'all 0.15s ease',
              }}
              onClick={() => {
                item.onClick()
                close()
              }}
              onMouseEnter={(e) => {
                e.currentTarget.style.backgroundColor = item.danger
                  ? 'var(--color-danger-subtle)'
                  : 'var(--color-surface-hover)'
                if (!item.danger) e.currentTarget.style.color = 'var(--color-text-primary)'
              }}
              onMouseLeave={(e) => {
                e.currentTarget.style.backgroundColor = 'transparent'
                e.currentTarget.style.color = item.danger
                  ? 'var(--color-danger)'
                  : 'var(--color-text-secondary)'
              }}
            >
              {item.icon && <span className="flex items-center">{item.icon}</span>}
              <span className="flex-1 truncate">{item.label}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  )
}
