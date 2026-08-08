import { useState, useEffect, type FormEvent } from 'react'
import { toast } from 'sonner'
import { useTranslation } from 'react-i18next'
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
  type LucideIcon,
} from 'lucide-react'
import { getUserMe, updateUserMe, changePassword, getUserStats } from '../api/client'
import type { UserProfile, UserStats } from '../api/client'
import StorageConfigSection from '../components/StorageConfigSection'
import WatermarkSettings from '../components/WatermarkSettings'
import { PreprocessingSettings } from '../components/PreprocessingSettings'
import { useFormat } from '../hooks/useFormat'

type SettingsSection =
  | 'profile'
  | 'password'
  | 'storage-usage'
  | 'storage-configs'
  | 'watermark'
  | 'preprocessing'
  | 'oauth'

const SECTION_META: Record<
  SettingsSection,
  { titleKey: 'settings.profile' | 'settings.password' | 'settings.storageUsage' | 'settings.storageBackends' | 'settings.watermark' | 'settings.preprocessing' | 'settings.oauth'; icon: LucideIcon }
> = {
  profile: { titleKey: 'settings.profile', icon: User },
  password: { titleKey: 'settings.password', icon: Lock },
  'storage-usage': { titleKey: 'settings.storageUsage', icon: HardDrive },
  'storage-configs': { titleKey: 'settings.storageBackends', icon: Database },
  watermark: { titleKey: 'settings.watermark', icon: Droplets },
  preprocessing: { titleKey: 'settings.preprocessing', icon: Image },
  oauth: { titleKey: 'settings.oauth', icon: Shield },
}

const SECTION_ORDER: SettingsSection[] = [
  'profile',
  'password',
  'storage-usage',
  'storage-configs',
  'watermark',
  'preprocessing',
  'oauth',
]

function parseSectionFromHash(hash: string): SettingsSection | null {
  const match = hash.match(/^#settings\?section=([a-z-]+)/)
  if (!match) return null
  const section = match[1] as SettingsSection
  return section in SECTION_META ? section : null
}

export default function Settings() {
  const { t } = useTranslation()
  const { formatBytes } = useFormat()
  const [profile, setProfile] = useState<UserProfile | null>(null)
  const [stats, setStats] = useState<UserStats | null>(null)
  const [loading, setLoading] = useState(true)
  const [saving, setSaving] = useState(false)

  const [username, setUsername] = useState('')
  const [email, setEmail] = useState('')

  const [currentPassword, setCurrentPassword] = useState('')
  const [newPassword, setNewPassword] = useState('')
  const [changingPw, setChangingPw] = useState(false)

  const [activeSection, setActiveSection] = useState<SettingsSection>(() => {
    const section = parseSectionFromHash(window.location.hash)
    return section ?? 'profile'
  })

  useEffect(() => {
    Promise.all([getUserMe(), getUserStats()])
      .then(([p, s]) => {
        setProfile(p)
        setStats(s)
        setUsername(p.username)
        setEmail(p.email ?? '')
      })
      .catch(() => toast.error(t('settings.failedToLoad')))
      .finally(() => setLoading(false))
  }, [])

  function selectSection(id: SettingsSection) {
    setActiveSection(id)
    window.history.replaceState(null, '', `#settings?section=${id}`)
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
      toast.success(t('settings.profileUpdated'))
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('settings.failedToSave'))
    } finally {
      setSaving(false)
    }
  }

  async function handleChangePassword(e: FormEvent) {
    e.preventDefault()
    if (newPassword.length < 8) {
      toast.error(t('settings.passwordTooShort'))
      return
    }
    setChangingPw(true)
    try {
      await changePassword({ current_password: currentPassword, new_password: newPassword })
      toast.success(t('settings.passwordChanged'))
      setCurrentPassword('')
      setNewPassword('')
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('settings.failedToChangePassword'))
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
    <div className="mx-auto max-w-4xl p-4">
      <h2
        className="mb-4 text-lg font-semibold"
        style={{ color: 'var(--color-text-primary)', fontFamily: "'Outfit', system-ui, sans-serif" }}
      >
        {t('settings.title')}
      </h2>

      <div className="flex flex-col gap-4 md:flex-row">
        {/* Left: section list */}
        <nav className="md:w-52 md:shrink-0">
          <div className="glass flex gap-0.5 overflow-x-auto rounded-lg p-1 md:sticky md:top-4 md:flex-col">
            {SECTION_ORDER.map((id) => {
              const { titleKey, icon: Icon } = SECTION_META[id]
              const active = activeSection === id
              return (
                <button
                  key={id}
                  onClick={() => selectSection(id)}
                  aria-current={active ? 'page' : undefined}
                  className={`flex shrink-0 items-center gap-2 whitespace-nowrap rounded-md px-3 py-2 text-sm font-medium transition-all duration-200 md:w-full ${
                    active
                      ? 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)] shadow-sm'
                      : 'text-[var(--color-text-muted)] hover:text-[var(--color-text-secondary)]'
                  }`}
                >
                  <Icon className="h-4 w-4 shrink-0" />
                  {t(titleKey)}
                </button>
              )
            })}
          </div>
        </nav>

        {/* Right: content */}
        <div className="min-w-0 flex-1">
          {activeSection === 'storage-configs' ? (
            <StorageConfigSection />
          ) : activeSection === 'watermark' ? (
            <WatermarkSettings
              profile={profile}
              onUpdate={(updatedProfile) => setProfile(updatedProfile)}
            />
          ) : (
          <div className="glass rounded-lg p-4">
            {activeSection === 'profile' && (
              <form onSubmit={handleSaveProfile} className="space-y-3">
                <div className="grid gap-3 sm:grid-cols-2">
                  <div>
                    <label
                      className="mb-1 block text-xs font-medium"
                      style={{ color: 'var(--color-text-secondary)' }}
                    >
                      {t('settings.username')}
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
                      {t('settings.email')}
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
                  {t('settings.saveProfile')}
                </button>
              </form>
            )}

            {activeSection === 'password' && (
              <form onSubmit={handleChangePassword} className="space-y-3">
                <div className="grid gap-3 sm:grid-cols-2">
                  <div>
                    <label
                      className="mb-1 block text-xs font-medium"
                      style={{ color: 'var(--color-text-secondary)' }}
                    >
                      {t('settings.currentPassword')}
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
                      {t('settings.newPassword')}
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
                  {t('settings.changePassword')}
                </button>
              </form>
            )}

            {activeSection === 'storage-usage' && (
              <div className="space-y-3">
                <h3
                  className="text-sm font-medium"
                  style={{ color: 'var(--color-text-primary)' }}
                >
                  {t('settings.storageUsage')}
                </h3>
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
                    {formatBytes(used)} {t('settings.usedUnlimited')}
                  </p>
                )}
              </div>
            )}

            {activeSection === 'preprocessing' && <PreprocessingSettings />}

            {activeSection === 'oauth' && (
              <div>
                <h3
                  className="mb-2 text-sm font-medium"
                  style={{ color: 'var(--color-text-primary)' }}
                >
                  {t('settings.oauthAccounts')}
                </h3>
                <p className="mb-3 text-xs" style={{ color: 'var(--color-text-muted)' }}>
                  {t('settings.oauthHint')}
                </p>
                <div className="flex gap-2">
                  <a href="/api/v1/auth/oauth/github" className="btn-ghost text-xs">
                    {t('settings.linkGitHub')}
                  </a>
                  <a href="/api/v1/auth/oauth/google" className="btn-ghost text-xs">
                    {t('settings.linkGoogle')}
                  </a>
                </div>
              </div>
            )}
          </div>
          )}
        </div>
      </div>
    </div>
  )
}
