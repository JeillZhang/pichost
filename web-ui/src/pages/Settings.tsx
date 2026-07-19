import { useState, useEffect, type FormEvent } from 'react'
import { toast } from 'sonner'
import { Loader2, Save, Lock } from 'lucide-react'
import { getUserMe, updateUserMe, changePassword, getUserStats } from '../api/client'
import type { UserProfile, UserStats } from '../api/client'
import StorageConfigSection from '../components/StorageConfigSection'
import WatermarkSettings from '../components/WatermarkSettings'

export default function Settings() {
  const [profile, setProfile] = useState<UserProfile | null>(null)
  const [stats, setStats] = useState<UserStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)

  const [username, setUsername] = useState('')
  const [email, setEmail] = useState('')

  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [changingPw, setChangingPw] = useState(false)

  useEffect(() => {
    Promise.all([getUserMe(), getUserStats()])
      .then(([p, s]) => {
        setProfile(p)
        setStats(s)
        setUsername(p.username)
        setEmail(p.email ?? '')
      })
      .catch(() => toast.error('Failed to load profile'))
      .finally(() => setLoading(false))
  }, [])

  async function handleSaveProfile(e: FormEvent) {
    e.preventDefault()
    setSaving(true)
    try {
      const updated = await updateUserMe({
        username: username || undefined,
        email: email || undefined,
      })
      setProfile(updated)
      toast.success('Profile updated')
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Failed to save')
    } finally {
      setSaving(false)
    }
  }

  async function handleChangePassword(e: FormEvent) {
    e.preventDefault()
    if (newPassword.length < 8) {
      toast.error('Password must be at least 8 characters')
      return
    }
    setChangingPw(true)
    try {
      await changePassword({ current_password: currentPassword, new_password: newPassword })
      toast.success('Password changed')
      setCurrentPassword('')
      setNewPassword('')
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Failed to change password')
    } finally {
      setChangingPw(false)
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center p-8">
        <Loader2 className="h-6 w-6 animate-spin" style={{ color: 'var(--color-text-muted)' }} />
      </div>
    )
  }

  const used = stats?.total_size ?? 0
  const quota = profile?.storage_quota
  const usagePercent = quota && quota > 0 ? Math.min(100, (used / quota) * 100) : 0
  const quotaColor = usagePercent > 80 ? 'var(--color-danger)' : usagePercent > 50 ? '#eab308' : 'var(--color-accent)'

  return (
    <div className="mx-auto max-w-2xl space-y-4 p-4">
      <h2 className="text-lg font-semibold" style={{ color: 'var(--color-text-primary)' }}>Settings</h2>

      {/* Profile Card */}
      <form onSubmit={handleSaveProfile} className="space-y-3 rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-4 backdrop-blur-sm">
        <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>Profile</h3>
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Username</label>
            <input type="text" value={username} onChange={e => setUsername(e.target.value)}
              className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm"
              style={{ color: 'var(--color-text-primary)' }} />
          </div>
          <div>
            <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Email</label>
            <input type="email" value={email} onChange={e => setEmail(e.target.value)}
              className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm"
              style={{ color: 'var(--color-text-primary)' }} />
          </div>
        </div>
        <button type="submit" disabled={saving}
          className="flex items-center gap-2 rounded-lg px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50"
          style={{ backgroundColor: 'var(--color-accent)' }}>
          {saving ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Save className="h-3.5 w-3.5" />}
          Save Profile
        </button>
      </form>

      {/* Password Card */}
      <form onSubmit={handleChangePassword} className="space-y-3 rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-4 backdrop-blur-sm">
        <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>Password</h3>
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Current Password</label>
            <input type="password" required value={currentPassword} onChange={e => setCurrentPassword(e.target.value)}
              className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm"
              style={{ color: 'var(--color-text-primary)' }} />
          </div>
          <div>
            <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>New Password (min 8 chars)</label>
            <input type="password" required minLength={8} value={newPassword} onChange={e => setNewPassword(e.target.value)}
              className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm"
              style={{ color: 'var(--color-text-primary)' }} />
          </div>
        </div>
        <button type="submit" disabled={changingPw}
          className="flex items-center gap-2 rounded-lg px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50"
          style={{ backgroundColor: 'var(--color-accent)' }}>
          {changingPw ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Lock className="h-3.5 w-3.5" />}
          Change Password
        </button>
      </form>

      {/* Storage Usage */}
      <div className="space-y-3 rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-4 backdrop-blur-sm">
        <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>Storage Usage</h3>
        {quota && quota > 0 ? (
          <div>
            <div className="flex justify-between text-xs" style={{ color: 'var(--color-text-muted)' }}>
              <span>{formatBytes(used)} / {formatBytes(quota)}</span>
              <span>{usagePercent.toFixed(0)}%</span>
            </div>
            <div className="mt-1 h-2 overflow-hidden rounded-full" style={{ backgroundColor: 'var(--color-surface)' }}>
              <div className="h-full rounded-full transition-all" style={{ width: `${usagePercent}%`, backgroundColor: quotaColor }} />
            </div>
          </div>
        ) : (
          <p className="text-xs" style={{ color: 'var(--color-text-muted)' }}>{formatBytes(used)} used (unlimited)</p>
        )}
      </div>

      {/* Storage Configs */}
      <StorageConfigSection />

      {/* Watermark Settings */}
      <WatermarkSettings
        profile={profile}
        onUpdate={(updatedProfile) => setProfile(updatedProfile)}
      />

      {/* OAuth Card */}
      <div className="rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-4 backdrop-blur-sm">
        <h3 className="mb-2 text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>OAuth Accounts</h3>
        <p className="mb-3 text-xs" style={{ color: 'var(--color-text-muted)' }}>
          Link your GitHub or Google account for one-click login.
        </p>
        <div className="flex gap-2">
          <a href="/api/v1/auth/oauth/github"
            className="flex items-center gap-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-3 py-1.5 text-xs hover:bg-[var(--color-surface)] transition-colors"
            style={{ color: 'var(--color-text-primary)' }}>
            Link GitHub
          </a>
          <a href="/api/v1/auth/oauth/google"
            className="flex items-center gap-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-3 py-1.5 text-xs hover:bg-[var(--color-surface)] transition-colors"
            style={{ color: 'var(--color-text-primary)' }}>
            Link Google
          </a>
        </div>
      </div>
    </div>
  )
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`
  return `${(bytes / 1073741824).toFixed(2)} GB`
}
