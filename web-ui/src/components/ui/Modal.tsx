// ui/Modal.tsx
import { type ReactNode } from 'react'
import { X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import useOverlay from '../../hooks/useOverlay'

interface ModalProps {
  open: boolean
  onClose: () => void
  title?: string
  children: ReactNode
  footer?: ReactNode
  size?: 'sm' | 'md'
}

export default function Modal({
  open,
  onClose,
  title,
  children,
  footer,
  size = 'md',
}: ModalProps) {
  const { t } = useTranslation()
  const { overlayProps } = useOverlay(onClose)
  if (!open) return null

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center sm:items-center sm:p-4">
      <div
        data-testid="modal-overlay"
        {...overlayProps}
        className="absolute inset-0 bg-black/50 backdrop-blur-sm"
      />
      <div
        className={`glass-modal relative flex max-h-[90dvh] w-full flex-col overflow-hidden rounded-t-2xl sm:rounded-xl ${
          size === 'sm' ? 'sm:max-w-sm' : 'sm:max-w-md'
        }`}
      >
        {(title || footer) && (
          <div className="flex items-center justify-between px-5 pt-4">
            <h2
              className="text-lg font-semibold"
              style={{ color: 'var(--color-text-primary)', fontFamily: "'Outfit', system-ui, sans-serif" }}
            >
              {title}
            </h2>
            <button
              onClick={onClose}
              aria-label={t('modal.close')}
              className="rounded-lg p-1 transition-colors hover:bg-[var(--color-surface-hover)]"
              style={{ color: 'var(--color-text-muted)' }}
            >
              <X className="h-5 w-5" />
            </button>
          </div>
        )}
        <div className="overflow-y-auto px-5 py-4">{children}</div>
        {footer && (
          <div className="flex justify-end gap-3 border-t px-5 py-3" style={{ borderColor: 'var(--color-border)' }}>
            {footer}
          </div>
        )}
      </div>
    </div>
  )
}
