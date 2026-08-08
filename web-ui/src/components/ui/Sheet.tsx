import { type ReactNode } from 'react'
import { X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import useOverlay from '../../hooks/useOverlay'

interface SheetProps {
  open: boolean
  onClose: () => void
  title: string
  children: ReactNode
}

export default function Sheet({ open, onClose, title, children }: SheetProps) {
  const { t } = useTranslation()
  const { overlayProps } = useOverlay(onClose, open)
  if (!open) return null

  return (
    <>
      <div
        {...overlayProps}
        data-testid="sheet-overlay"
        className="fixed inset-0 z-40 bg-black/30 backdrop-blur-sm"
      />
      <div
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="glass-elevated fixed inset-y-0 left-0 z-50 flex w-[85vw] max-w-xs flex-col"
      >
        <div className="flex items-center justify-between px-4 py-3">
          <h2 className="text-sm font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            {title}
          </h2>
          <button
            onClick={onClose}
            aria-label={t('modal.close')}
            className="rounded p-1"
            style={{ color: 'var(--color-text-muted)' }}
          >
            <X className="h-5 w-5" />
          </button>
        </div>
        <div className="min-h-0 flex-1 overflow-y-auto px-2 pb-4">{children}</div>
      </div>
    </>
  )
}
