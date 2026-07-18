import { useState, type FormEvent } from 'react'
import { toast } from 'sonner'
import { X } from 'lucide-react'
import api from '../api/client'
import type { UserInfo } from '../api/client'

interface EditUserDialogProps {
  user: UserInfo
  onClose: () => void
  onUpdated: () => void
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  return `${(bytes / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`
}

export default function EditUserDialog({ user, onClose, onUpdated }: EditUserDialogProps) {
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
      toast.success('User updated')
      onUpdated()
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Update failed'
      toast.error(msg)
    } finally {
      setSaving(false)
    }
  }

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
            Edit User
          </h2>
          <button onClick={onClose} className="rounded p-1" style={{ color: 'var(--color-text-muted)' }}>
            <X className="h-5 w-5" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          <div>
            <label className="mb-1 block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>
              Username
            </label>
            <input
              type="text"
              required
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                color: 'var(--color-text-primary)',
              }}
            />
          </div>

          <div>
            <label className="mb-1 block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>
              Email
            </label>
            <input
              type="email"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
              className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                color: 'var(--color-text-primary)',
              }}
            />
          </div>

          <div>
            <label className="mb-1 block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>
              New Password (leave blank to keep current)
            </label>
            <input
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              minLength={8}
              placeholder="••••••••"
              className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                color: 'var(--color-text-primary)',
              }}
            />
          </div>

          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={isAdmin}
              onChange={(e) => setIsAdmin(e.target.checked)}
              className="rounded"
            />
            <span className="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
              Admin privileges
            </span>
          </label>

          <div>
            <label className="mb-1 block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>
              Storage Quota
            </label>
            <input
              type="number"
              min={0}
              value={storageQuota ?? 0}
              onChange={(e) => {
                const v = e.target.value ? Number(e.target.value) : 0
                setStorageQuota(v > 0 ? v : null)
              }}
              className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
              style={{
                backgroundColor: 'var(--color-surface)',
                border: '1px solid var(--color-border)',
                color: 'var(--color-text-primary)',
              }}
            />
            <p className="mt-1 text-xs" style={{ color: 'var(--color-text-muted)' }}>
              {storageQuota != null && storageQuota > 0 ? formatBytes(storageQuota) : 'Unlimited'}
            </p>
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
              type="submit"
              disabled={saving}
              className="rounded-lg px-4 py-2 text-sm font-medium text-white disabled:opacity-50"
              style={{ backgroundColor: 'var(--color-accent)' }}
            >
              {saving ? 'Saving…' : 'Save'}
            </button>
          </div>
        </form>
      </div>
    </div>
  )
}
