// ui/ConfirmDialog.tsx
import { useTranslation } from 'react-i18next'
import Modal from './Modal'

interface ConfirmDialogProps {
  open: boolean
  onClose: () => void
  onConfirm: () => void
  title: string
  message: string
  confirmLabel: string
  cancelLabel?: string
  danger?: boolean
  pending?: boolean
}

export default function ConfirmDialog({
  open,
  onClose,
  onConfirm,
  title,
  message,
  confirmLabel,
  cancelLabel,
  danger = false,
  pending = false,
}: ConfirmDialogProps) {
  const { t } = useTranslation()
  return (
    <Modal open={open} onClose={onClose} title={title} size="sm">
      <p className="text-sm leading-relaxed" style={{ color: 'var(--color-text-secondary)' }}>
        {message}
      </p>
      <div className="mt-5 flex justify-end gap-3">
        <button onClick={onClose} disabled={pending} className="btn-ghost">
          {cancelLabel ?? t('common.cancel')}
        </button>
        <button
          onClick={onConfirm}
          disabled={pending}
          className="btn-accent"
          style={danger ? { background: 'var(--color-danger)', color: 'white' } : undefined}
        >
          {confirmLabel}
        </button>
      </div>
    </Modal>
  )
}
