import { useState, useEffect, type FormEvent, type ReactNode } from 'react'
import { toast } from 'sonner'
import {
  Loader2,
  Save,
  Lock,
  User,
  HardDrive,
  Database,
  Droplets,
  Image,
  Shield,
  ChevronDown,
  type LucideIcon,
} from 'lucide-react'
import { getUserMe, updateUserMe, changePassword, getUserStats } from '../api/client'
import type { UserProfile, UserStats } from '../api/client'
import StorageConfigSection from '../components/StorageConfigSection'
import WatermarkSettings from '../components/WatermarkSettings'
import { PreprocessingSettings } from '../components/PreprocessingSettings'

type SettingsSection =
  | 'profile'
  | 'password'
  | 'storage-usage'
  | 'storage-configs'
  | 'watermark'
  | 'preprocessing'
  | 'oauth'

const SECTION_META: Record<SettingsSection, { title: string; icon: LucideIcon }> = {
  profile: { title: 'Profile', icon: User },
  password: { title: 'Password', icon: Lock },
  'storage-usage': { title: 'Storage Usage', icon: HardDrive },
  'storage-configs': { title: 'Storage Backends', icon: Database },
  watermark: { title: 'Watermark', icon: Droplets },
  preprocessing: { title: 'Preprocessing', icon: Image },
  oauth: { title: 'OAuth', icon: Shield },
}

const MOBILE_QUERY = '(max-width: 767px)'

function parseSectionFromHash(hash: string): SettingsSection | null {
  const match = hash.match(/^#settings\?section=([a-z-]+)/)
  if (!match) return null
  const section = match[1] as SettingsSection
  return section in SECTION_META ? section : null
}

interface AccordionSectionProps {
  id: SettingsSection
  expanded: boolean
  onToggle: (id: SettingsSection) => void
  padded?: boolean
  children: ReactNode
}

function AccordionSection({ id, expanded, onToggle, padded = true, children }: AccordionSectionProps) {
  const { title, icon: Icon } = SECTION_META[id]
  return (
    <div className="glass overflow-hidden rounded-lg">
      <button
        type="button"
        onClick={() => onToggle(id)}
        aria-expanded={expanded}
        aria-controls={`settings-section-${id}-content`}
        className="flex w-full cursor-pointer items-center gap-2.5 px-4 py-3 text-left transition-colors duration-150 hover:bg-[var(--glass-bg-hover)]"
      >
        <Icon className="h-4 w-4 shrink-0" style={{ color: 'var(--color-text-muted)' }} />
        <span
          className="text-sm font-medium"
          style={{ color: 'var(--color-text-primary)' }}
        >
          {title}
        </span>
        <ChevronDown
          className={`ml-auto h-4 w-4 shrink-0 transition-transform duration-200 ${expanded ? 'rotate-180' : ''}`}
          style={{ color: 'var(--color-text-muted)' }}
        />
      </button>
      {expanded && (
        <div
          id={`settings-section-${id}-content`}
          className="border-t border-[var(--glass-border)]"
          style={{ padding: padded ? '1rem' : '0' }}
        >
          {children}
        </div>
      )}
    </div>
  )
}

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

  const [expanded, setExpanded] = useState<Set<SettingsSection>>(() => {
    const section = parseSectionFromHash(window.location.hash)
    return section ? new Set([section]) : new Set()
  })
  const [scrollTarget, setScrollTarget] = useState<SettingsSection | null>(() =>
    parseSectionFromHash(window.location.hash),
  )

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

  useEffect(() => {
    if (loading || !scrollTarget) return
    requestAnimationFrame(() => {
      document
        .getElementById(`settings-section-${scrollTarget}`)
        ?.scrollIntoView({ behavior: 'smooth', block: 'start' })
      setScrollTarget(null)
    })
  }, [loading, scrollTarget])

  function toggleSection(id: SettingsSection) {
    const isMobile = window.matchMedia(MOBILE_QUERY).matches
    const next = new Set(expanded)
    if (next.has(id)) {
      next.delete(id)
    } else {
      if (isMobile) next.clear()
      next.add(id)
    }
    setExpanded(next)
    if (next.has(id)) {
      window.history.replaceState(null, '', `#settings?section=${id}`)
    } else if (next.size === 0) {
      window.history.replaceState(null, '', window.location.pathname + window.location.search)
    }
  }

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
  const quotaColor =
    usagePercent > 80
      ? 'var(--color-danger)'
      : usagePercent > 50
        ? 'var(--color-warning)'
        : 'var(--color-accent)'

  return (
    <div className="mx-auto max-w-2xl space-y-3 p-4">
      <h2
        className="text-lg font-semibold"
        style={{ color: 'var(--color-text-primary)', fontFamily: "'Outfit', system-ui, sans-serif" }}
      >
        Settings
      </h2>

      {/* Profile */}
      <AccordionSection id="profile" expanded={expanded.has('profile')} onToggle={toggleSection}>
        <form onSubmit={handleSaveProfile} className="space-y-3">
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label
                className="mb-1 block text-xs font-medium"
                style={{ color: 'var(--color-text-secondary)' }}
              >
                Username
              </label>
              <input
                type="text"
                value={username}
                onChange={(e) => setUsername(e.target.value)}
                className="input-field"
              />
            </div>
            <div>
              <label
                className="mb-1 block text-xs font-medium"
                style={{ color: 'var(--color-text-secondary)' }}
              >
                Email
              </label>
              <input
                type="email"
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                className="input-field"
              />
            </div>
          </div>
          <button type="submit" disabled={saving} className="btn-accent">
            {saving ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Save className="h-3.5 w-3.5" />}
            Save Profile
          </button>
        </form>
      </AccordionSection>

      {/* Password */}
      <AccordionSection id="password" expanded={expanded.has('password')} onToggle={toggleSection}>
        <form onSubmit={handleChangePassword} className="space-y-3">
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <label
                className="mb-1 block text-xs font-medium"
                style={{ color: 'var(--color-text-secondary)' }}
              >
                Current Password
              </label>
              <input
                type="password"
                required
                value={currentPassword}
                onChange={(e) => setCurrentPassword(e.target.value)}
                className="input-field"
              />
            </div>
            <div>
              <label
                className="mb-1 block text-xs font-medium"
                style={{ color: 'var(--color-text-secondary)' }}
              >
                New Password (min 8 chars)
              </label>
              <input
                type="password"
                required
                minLength={8}
                value={newPassword}
                onChange={(e) => setNewPassword(e.target.value)}
                className="input-field"
              />
            </div>
          </div>
          <button type="submit" disabled={changingPw} className="btn-accent">
            {changingPw ? (
              <Loader2 className="h-3.5 w-3.5 animate-spin" />
            ) : (
              <Lock className="h-3.5 w-3.5" />
            )}
            Change Password
          </button>
        </form>
      </AccordionSection>

      {/* Storage Usage */}
      <AccordionSection
        id="storage-usage"
        expanded={expanded.has('storage-usage')}
        onToggle={toggleSection}
      >
        <div className="space-y-3">
          {quota && quota > 0 ? (
            <div>
              <div
                className="mb-1.5 flex justify-between text-xs"
                style={{ color: 'var(--color-text-muted)' }}
              >
                <span>
                  {formatBytes(used)} / {formatBytes(quota)}
                </span>
                <span>{usagePercent.toFixed(0)}%</span>
              </div>
              <div
                className="h-2 overflow-hidden rounded-full"
                style={{ backgroundColor: 'var(--color-surface)' }}
              >
                <div
                  className="h-full rounded-full transition-all"
                  style={{ width: `${usagePercent}%`, backgroundColor: quotaColor }}
                />
              </div>
            </div>
          ) : (
            <p className="text-xs" style={{ color: 'var(--color-text-muted)' }}>
              {formatBytes(used)} used (unlimited)
            </p>
          )}
        </div>
      </AccordionSection>

      {/* Storage Configs */}
      <AccordionSection
        id="storage-configs"
        expanded={expanded.has('storage-configs')}
        onToggle={toggleSection}
        padded={false}
      >
        <StorageConfigSection />
      </AccordionSection>

      {/* Watermark Settings */}
      <AccordionSection
        id="watermark"
        expanded={expanded.has('watermark')}
        onToggle={toggleSection}
        padded={false}
      >
        <WatermarkSettings
          profile={profile}
          onUpdate={(updatedProfile) => setProfile(updatedProfile)}
        />
      </AccordionSection>

      {/* Preprocessing Settings */}
      <AccordionSection
        id="preprocessing"
        expanded={expanded.has('preprocessing')}
        onToggle={toggleSection}
      >
        <PreprocessingSettings />
      </AccordionSection>

      {/* OAuth */}
      <AccordionSection id="oauth" expanded={expanded.has('oauth')} onToggle={toggleSection}>
        <h3
          className="mb-2 text-sm font-medium"
          style={{ color: 'var(--color-text-primary)' }}
        >
          OAuth Accounts
        </h3>
        <p className="mb-3 text-xs" style={{ color: 'var(--color-text-muted)' }}>
          Link your GitHub or Google account for one-click login.
        </p>
        <div className="flex gap-2">
          <a
            href="/api/v1/auth/oauth/github"
            className="btn-ghost text-xs"
          >
            Link GitHub
          </a>
          <a
            href="/api/v1/auth/oauth/google"
            className="btn-ghost text-xs"
          >
            Link Google
          </a>
        </div>
      </AccordionSection>
    </div>
  )
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1048576) return `${(bytes / 1024).toFixed(1)} KB`
  if (bytes < 1073741824) return `${(bytes / 1048576).toFixed(1)} MB`
  return `${(bytes / 1073741824).toFixed(2)} GB`
}
