import { useState } from 'react'
import { toast } from 'sonner'
import { X, Copy, Check } from 'lucide-react'
import { createInviteCode } from '../api/client'

interface CreateInviteDialogProps {
  onClose: () => void
  onCreated: () => void
}

export default function CreateInviteDialog({ onClose, onCreated }: CreateInviteDialogProps) {
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
      const msg = e instanceof Error ? e.message : 'Failed to create invite code'
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
      toast.error('Failed to copy')
    }
  }

  function handleDone() {
    onCreated()
  }

  // Phase 2 — success
  if (code !== null) {
    return (
      <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
        <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" onClick={handleDone} />

        <div
          className="relative w-full max-w-md rounded-xl p-6"
          style={{
            backgroundColor: 'var(--color-surface-elevated)',
            border: '1px solid var(--glass-border)',
            backdropFilter: 'blur(var(--glass-blur))',
            boxShadow: 'var(--glass-shadow)',
          }}
        >
          <div className="mb-4 flex items-center justify-between">
            <h2 className="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
              Invite Code Created
            </h2>
            <button onClick={handleDone} className="rounded p-1" style={{ color: 'var(--color-text-muted)' }}>
              <X className="h-5 w-5" />
            </button>
          </div>

          <div className="space-y-4">
            <p className="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
              Share this code with the person you want to invite. It expires in <strong>{ttlDays}</strong> day{ttlDays !== 1 ? 's' : ''}.
            </p>

            <div
              className="rounded-lg px-4 py-3 font-mono text-sm select-all break-all"
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                color: 'var(--color-text-primary)',
              }}
            >
              {code}
            </div>

            <button
              onClick={handleCopy}
              className="flex w-full items-center justify-center gap-2 rounded-lg px-4 py-2 text-sm font-medium text-white transition-colors"
              style={{ backgroundColor: 'var(--color-accent)' }}
            >
              {copied ? (
                <>
                  <Check className="h-4 w-4" />
                  Copied
                </>
              ) : (
                <>
                  <Copy className="h-4 w-4" />
                  Copy Code
                </>
              )}
            </button>

            <div className="flex justify-end pt-2">
              <button
                onClick={handleDone}
                className="rounded-lg px-4 py-2 text-sm font-medium text-white"
                style={{ backgroundColor: 'var(--color-accent)' }}
              >
                Done
              </button>
            </div>
          </div>
        </div>
      </div>
    )
  }

  // Phase 1 — form
  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" onClick={onClose} />

      <div
        className="relative w-full max-w-md rounded-xl p-6"
        style={{
          backgroundColor: 'var(--color-surface-elevated)',
          border: '1px solid var(--glass-border)',
          backdropFilter: 'blur(var(--glass-blur))',
          boxShadow: 'var(--glass-shadow)',
        }}
      >
        <div className="mb-4 flex items-center justify-between">
          <h2 className="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>
            Create Invite Code
          </h2>
          <button onClick={onClose} className="rounded p-1" style={{ color: 'var(--color-text-muted)' }}>
            <X className="h-5 w-5" />
          </button>
        </div>

        <div className="space-y-4">
          <div>
            <label className="mb-1 block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>
              Expires in (days)
            </label>
            <input
              type="number"
              required
              min={1}
              max={90}
              value={ttlDays}
              onChange={(e) => setTtlDays(Math.max(1, Math.min(90, Number(e.target.value) || 1)))}
              className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                color: 'var(--color-text-primary)',
              }}
            />
          </div>

          <div className="flex justify-end gap-3 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="rounded-lg px-4 py-2 text-sm transition-colors"
              style={{ color: 'var(--color-text-muted)' }}
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={handleCreate}
              disabled={creating}
              className="rounded-lg px-4 py-2 text-sm font-medium text-white disabled:opacity-50"
              style={{ backgroundColor: 'var(--color-accent)' }}
            >
              {creating ? 'Creating…' : 'Create'}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}
