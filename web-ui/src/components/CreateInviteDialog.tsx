import { useState } from 'react'
import { toast } from 'sonner'
import { Copy, Check } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import Modal from './ui/Modal'
import { createInviteCode } from '../api/client'

interface CreateInviteDialogProps {
  onClose: () => void
  onCreated: () => void
}

export default function CreateInviteDialog({ onClose, onCreated }: CreateInviteDialogProps) {
  const { t } = useTranslation()
  const [ttlDays, setTtlDays] = useState(7)
  const [creating, setCreating] = useState(false)
  const [code, setCode] = useState<string | null>(null)
  const [copied, setCopied] = useState(false)

  async function handleCreate() {
    setCreating(true)
    try {
      const res = await createInviteCode(ttlDays)
      setCode(res.code)
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : t('createInvite.createFailed')
      toast.error(msg)
    } finally {
      setCreating(false)
    }
  }

  async function handleCopy() {
    if (!code) return
    try {
      await navigator.clipboard.writeText(code)
      setCopied(true)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      toast.error(t('createInvite.copyFailed'))
    }
  }

  function handleDone() {
    onCreated()
  }

  // Phase 2 — success
  if (code !== null) {
    return (
      <Modal
        open
        onClose={handleDone}
        title={t('createInvite.created')}
      >
        <div className="space-y-4">
          <p className="text-sm leading-relaxed" style={{ color: 'var(--color-text-secondary)' }}>
            {t('createInvite.shareHint', {
              count: ttlDays,
              days: t('createInvite.dayLabel', { count: ttlDays }),
            })}
          </p>

          <div
            className="rounded-lg px-4 py-3 font-mono text-sm select-all break-all"
            style={{
              backgroundColor: 'var(--color-surface)',
              border: '1px solid var(--glass-border-base)',
              color: 'var(--color-text-primary)',
            }}
          >
            {code}
          </div>

          <button onClick={handleCopy} className="btn-accent w-full">
            {copied ? (
              <>
                <Check className="h-4 w-4" />
                {t('createInvite.copied')}
              </>
            ) : (
              <>
                <Copy className="h-4 w-4" />
                {t('createInvite.copyCode')}
              </>
            )}
          </button>

          <div className="flex justify-end pt-2">
            <button onClick={handleDone} className="btn-ghost">
              {t('createInvite.done')}
            </button>
          </div>
        </div>
      </Modal>
    )
  }

  // Phase 1 — form
  return (
    <Modal open onClose={onClose} title={t('createInvite.title')}>
      <div className="space-y-4">
        <div>
          <label
            className="mb-1.5 block text-xs font-medium"
            style={{ color: 'var(--color-text-secondary)' }}
          >
            {t('createInvite.expiresIn')}
          </label>
          <input
            type="number"
            required
            min={1}
            max={90}
            value={ttlDays}
            onChange={(e) => setTtlDays(Math.max(1, Math.min(90, Number(e.target.value) || 1)))}
            className="input-field"
          />
        </div>

        <div className="flex justify-end gap-3 pt-2">
          <button type="button" onClick={onClose} className="btn-ghost">
            {t('createInvite.cancel')}
          </button>
          <button
            type="button"
            onClick={handleCreate}
            disabled={creating}
            className="btn-accent"
          >
            {creating ? t('createInvite.creating') : t('createInvite.create')}
          </button>
        </div>
      </div>
    </Modal>
  )
}
