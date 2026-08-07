import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import { X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useFormat } from '../hooks/useFormat'
import api from '../api/client'
import type { UserInfo } from '../api/client'

interface EditUserDialogProps {
  user: UserInfo
  onClose: () => void
  onUpdated: () => void
}

export default function EditUserDialog({ user, onClose, onUpdated }: EditUserDialogProps) {
  const { t } = useTranslation()
  const { formatBytes } = useFormat()
  const [username, setUsername] = useState(user.username)
  const [email, setEmail] = useState(user.email ?? '')
  const [isAdmin, setIsAdmin] = useState(user.is_admin)
  const [password, setPassword] = useState('')
  const [storageQuota, setStorageQuota] = useState<number | null>(user.storage_quota)
  const [saving, setSaving] = useState(false)

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    setSaving(true)
    try {
      const body: Record<string, unknown> = { username }
      if (email) body.email = email
      if (password) body.password = password
      body.is_admin = isAdmin
      body.storage_quota = storageQuota

      await api.patch(`admin/users/${user.id}`, { json: body }).json()
      toast.success(t('editUser.userUpdated'))
      onUpdated()
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : t('editUser.updateFailed')
      toast.error(msg)
    } finally {
      setSaving(false)
    }
  }

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" onClick={onClose} />
      <div className="glass-modal relative w-full max-w-md p-6">
        <div className="mb-4 flex items-center justify-between">
          <h2
            className="text-lg font-semibold"
            style={{ color: 'var(--color-text-primary)', fontFamily: "'Outfit', system-ui, sans-serif" }}
          >
            {t('editUser.title')}
          </h2>
          <button
            onClick={onClose}
            className="rounded-lg p-1 transition-colors hover:bg-[var(--color-surface-hover)]"
            style={{ color: 'var(--color-text-muted)' }}
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label
              className="mb-1.5 block text-xs font-medium"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              {t('editUser.username')}
            </label>
            <input
              type="text"
              required
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="input-field"
            />
          </div>

          <div>
            <label
              className="mb-1.5 block text-xs font-medium"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              {t('editUser.email')}
            </label>
            <input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              className="input-field"
            />
          </div>

          <div>
            <label
              className="mb-1.5 block text-xs font-medium"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              {t('editUser.newPassword')}
            </label>            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              minLength={8}
              placeholder="••••••••"
              className="input-field"
            />
          </div>

          <label className="flex items-center gap-2.5">
            <input
              type="checkbox"
              checked={isAdmin}
              onChange={(e) => setIsAdmin(e.target.checked)}
              className="h-4 w-4 rounded accent-[var(--color-accent)]"
            />
            <span className="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
              {t('editUser.adminPrivileges')}
            </span>
          </label>

          <div>
            <label
              className="mb-1.5 block text-xs font-medium"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              {t('editUser.storageQuota')}
            </label>
            <input
              type="number"
              min={0}
              value={storageQuota ?? 0}
              onChange={(e) => {
                const v = e.target.value ? Number(e.target.value) : 0
                setStorageQuota(v > 0 ? v : null)
              }}
              className="input-field"
            />
            <p className="mt-1 text-xs" style={{ color: 'var(--color-text-muted)' }}>
              {storageQuota != null && storageQuota > 0 ? formatBytes(storageQuota) : t('editUser.unlimited')}
            </p>
          </div>

          <div className="flex justify-end gap-3 pt-2">
            <button type="button" onClick={onClose} className="btn-ghost">
              {t('editUser.cancel')}
            </button>
            <button type="submit" disabled={saving} className="btn-accent">
              {saving ? t('editUser.saving') : t('editUser.save')}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
