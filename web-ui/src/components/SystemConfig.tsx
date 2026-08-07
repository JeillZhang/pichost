import { useCallback, useEffect, useState } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import {
  Archive,
  Check,
  Database,
  Info,
  KeyRound,
  Loader2,
  RefreshCw,
  RotateCcw,
  Save,
  Server,
  X,
} from 'lucide-react'
import api from '../api/client'
import Button from './ui/Button'
import GlassSelect from './ui/GlassSelect'

interface ConfigResponse {
  database_url: string
  redis_url: string
  jwt_secret: string
  token_encryption_key: string
  public_url: string
  default_backend: string
  local_base_path: string
  config_path: string
}

interface TestResult {
  database: string | null
  redis: string | null
}

interface BackupInfo {
  filename: string
}

const BACKEND_OPTIONS = ['local', 'rustfs', 'github', 'gitcode']

const inputClass =
  'w-full rounded-lg border border-[var(--glass-border-base)] bg-[var(--glass-tint-base)]/65 px-3 py-1.5 text-sm text-[var(--color-text-primary)] focus:outline-none focus:ring-1 focus:ring-[var(--color-accent)] disabled:opacity-50'

const cardClass =
  'rounded-lg border border-[var(--color-border)] bg-[var(--glass-tint-base)]/65 p-4 backdrop-blur-sm'

function TestStatus({ result }: { result: string | null }) {
  const { t } = useTranslation()
  if (!result) return null
  const ok = result === 'ok'
  return (
    <p
      className="mt-1 flex items-center gap-1 text-xs"
      style={{ color: ok ? 'var(--color-success)' : 'var(--color-danger)' }}
    >
      {ok ? <Check className="h-3 w-3" /> : <X className="h-3 w-3" />}
      {ok ? t('systemConfig.connectionOk') : result.replace(/^fail:\s*/, '')}
    </p>
  )
}

function CardHeader({ icon: Icon, title }: { icon: typeof Database; title: string }) {
  return (
    <div className="mb-3 flex items-center gap-2">
      <Icon className="h-4 w-4" style={{ color: 'var(--color-text-muted)' }} />
      <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>{title}</h3>
    </div>
  )
}

function FieldLabel({ children }: { children: React.ReactNode }) {
  return (
    <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>
      {children}
    </label>
  )
}

/** Explains where the selected backend's own credentials are configured.
 *  Git providers are per-user storage configs; RustFS lives in config.toml. */
function BackendConfigHint({ backend }: { backend: string }) {
  const { t } = useTranslation()
  const hint: Record<string, React.ReactNode> = {
    rustfs: (
      <>
        {t('systemConfig.backendHintRustfsBefore')}{' '}
        <code className="rounded bg-[var(--color-surface)] px-1 py-0.5 text-[11px]">
          {t('systemConfig.backendHintRustfsCode')}
        </code>{' '}
        {t('systemConfig.backendHintRustfsAfter')}
      </>
    ),
    github: (
      <>
        {t('systemConfig.backendHintGitHubBefore')}{' '}
        <span className="font-medium" style={{ color: 'var(--color-text-primary)' }}>
          {t('systemConfig.backendHintSettingsLink')}
        </span>{' '}
        {t('systemConfig.backendHintGitAfter')}
      </>
    ),
    gitcode: (
      <>
        {t('systemConfig.backendHintGitCodeBefore')}{' '}
        <span className="font-medium" style={{ color: 'var(--color-text-primary)' }}>
          {t('systemConfig.backendHintSettingsLink')}
        </span>{' '}
        {t('systemConfig.backendHintGitAfter')}
      </>
    ),
  }
  return (
    <div
      className="mt-1 flex items-start gap-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-2 text-xs leading-relaxed"
      style={{ color: 'var(--color-text-muted)' }}
    >
      <Info className="mt-0.5 h-3.5 w-3.5 shrink-0" style={{ color: 'var(--color-accent)' }} />
      <span>{hint[backend] ?? null}</span>
    </div>
  )
}

export default function SystemConfig() {
  const { t } = useTranslation()
  const [config, setConfig] = useState<ConfigResponse | null>(null)
  const [dirty, setDirty] = useState<Partial<ConfigResponse>>({})
  const [loading, setLoading] = useState(true)
  const [loadError, setLoadError] = useState<string | null>(null)
  const [saving, setSaving] = useState(false)
  const [testing, setTesting] = useState<'database' | 'redis' | null>(null)
  const [dbResult, setDbResult] = useState<string | null>(null)
  const [redisResult, setRedisResult] = useState<string | null>(null)
  const [backups, setBackups] = useState<BackupInfo[]>([])
  const [backingUp, setBackingUp] = useState(false)
  const [restoring, setRestoring] = useState<string | null>(null)

  const fetchConfig = useCallback(async () => {
    setLoading(true)
    setLoadError(null)
    try {
      const cfg = await api.get('admin/config').json<ConfigResponse>()
      setConfig(cfg)
      setDirty({})
    } catch (e: unknown) {
      setLoadError(e instanceof Error ? e.message : t('systemConfig.failedToLoadConfig'))
    } finally {
      setLoading(false)
    }
  }, [])

  const fetchBackups = useCallback(async () => {
    try {
      const list = await api.get('admin/config/backups').json<BackupInfo[]>()
      setBackups(list)
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('systemConfig.failedToLoadBackups'))
    }
  }, [])

  useEffect(() => {
    void fetchConfig()
    void fetchBackups()
  }, [fetchConfig, fetchBackups])

  const updateField = useCallback(<K extends keyof ConfigResponse>(key: K, value: string) => {
    setConfig(prev => (prev ? { ...prev, [key]: value } : prev))
    setDirty(prev => ({ ...prev, [key]: value }))
  }, [])

  async function handleTest(target: 'database' | 'redis') {
    if (!config) return
    setTesting(target)
    setDbResult(null)
    setRedisResult(null)
    const key = target === 'database' ? 'database_url' : 'redis_url'
    const body = target === 'database' ? { database_url: config[key] } : { redis_url: config[key] }
    try {
      const result = await api.post('admin/config/test', { json: body }).json<TestResult>()
      const status = result[target]
      if (status === 'ok') {
        if (target === 'database') setDbResult('ok')
        else setRedisResult('ok')
        toast.success(t('systemConfig.connectionOk'))
      } else {
        const msg = status?.replace(/^fail:\s*/, '') ?? t('systemConfig.connectionFailed')
        if (target === 'database') setDbResult(`fail: ${msg}`)
        else setRedisResult(`fail: ${msg}`)
        toast.error(msg)
      }
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : t('systemConfig.connectionTestFailed')
      if (target === 'database') setDbResult(`fail: ${msg}`)
      else setRedisResult(`fail: ${msg}`)
      toast.error(msg)
    } finally {
      setTesting(null)
    }
  }

  async function handleSave() {
    const changed = Object.keys(dirty)
    if (changed.length === 0) {
      toast.info(t('systemConfig.noChangesToSave'))
      return
    }
    setSaving(true)
    try {
      const updated = await api.put('admin/config', { json: dirty }).json<ConfigResponse>()
      setConfig(updated)
      setDirty({})
      toast.success(t('systemConfig.configSaved'))
      void fetchBackups()
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('systemConfig.failedToSaveConfig'))
    } finally {
      setSaving(false)
    }
  }

  async function handleBackup() {
    setBackingUp(true)
    try {
      const { filename } = await api.post('admin/config/backup').json<BackupInfo>()
      toast.success(t('systemConfig.configBackedUp', { filename }))
      void fetchBackups()
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('systemConfig.backupFailed'))
    } finally {
      setBackingUp(false)
    }
  }

  async function handleRestore(filename: string) {
    if (!window.confirm(t('systemConfig.restoreConfirm', { filename }))) return
    setRestoring(filename)
    try {
      const res = await api
        .post('admin/config/restore', { json: { backup_file: filename } })
        .json<{ status: string; from: string }>()
      toast.success(t('systemConfig.configRestored', { from: res.from }))
      await fetchConfig()
      setDbResult(null)
      setRedisResult(null)
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('systemConfig.restoreFailed'))
    } finally {
      setRestoring(null)
    }
  }

  if (loadError) {
    return (
      <div className="flex flex-col items-center gap-3 py-20">
        <p className="text-sm" style={{ color: 'var(--color-danger)' }}>{loadError}</p>
        <Button variant="ghost" size="sm" onClick={() => void fetchConfig()}>{t('systemConfig.retry')}</Button>
      </div>
    )
  }

  if (loading || !config) {
    return (
      <div className="flex items-center justify-center py-20" style={{ color: 'var(--color-text-muted)' }}>
        {t('systemConfig.loading')}
      </div>
    )
  }

  const changedCount = Object.keys(dirty).length

  return (
    <div className="space-y-4">
      <div className={cardClass}>
        <CardHeader icon={Database} title={t('systemConfig.database')} />
        <div>
          <FieldLabel>{t('systemConfig.postgresqlUrl')}</FieldLabel>
          <div className="mt-1 flex gap-2">
            <input
              type="text"
              value={config.database_url}
              onChange={e => updateField('database_url', e.target.value)}
              placeholder="postgres://user:pass@host:5432/db"
              className={inputClass}
            />
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void handleTest('database')}
              disabled={testing !== null}
              className="shrink-0"
            >
              {testing === 'database' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
              {t('systemConfig.testConnection')}
            </Button>
          </div>
          <TestStatus result={dbResult} />
        </div>
      </div>

      <div className={cardClass}>
        <CardHeader icon={Server} title={t('systemConfig.redis')} />
        <div>
          <FieldLabel>{t('systemConfig.redisUrl')}</FieldLabel>
          <div className="mt-1 flex gap-2">
            <input
              type="text"
              value={config.redis_url}
              onChange={e => updateField('redis_url', e.target.value)}
              placeholder="redis://host:6379"
              className={inputClass}
            />
            <Button
              variant="ghost"
              size="sm"
              onClick={() => void handleTest('redis')}
              disabled={testing !== null}
              className="shrink-0"
            >
              {testing === 'redis' ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
              {t('systemConfig.testConnection')}
            </Button>
          </div>
          <TestStatus result={redisResult} />
        </div>
      </div>

      <div className={cardClass}>
        <CardHeader icon={KeyRound} title={t('systemConfig.server')} />
        <div className="space-y-3">
          <div>
            <FieldLabel>{t('systemConfig.publicUrl')}</FieldLabel>
            <input
              type="text"
              value={config.public_url}
              onChange={e => updateField('public_url', e.target.value)}
              placeholder="https://pichost.example.com"
              className={`mt-1 ${inputClass}`}
            />
          </div>
          <div className="grid gap-3 sm:grid-cols-2">
            <div>
              <FieldLabel>{t('systemConfig.defaultStorageBackend')}</FieldLabel>
              <GlassSelect
                value={config.default_backend}
                onChange={(v) => updateField('default_backend', v)}
                options={BACKEND_OPTIONS.map((b) => ({ value: b, label: b }))}
                className="mt-1"
              />
            </div>
            <div>
              {config.default_backend === 'local' ? (
                <>
                  <FieldLabel>{t('systemConfig.localStoragePath')}</FieldLabel>
                  <input
                    type="text"
                    value={config.local_base_path}
                    onChange={e => updateField('local_base_path', e.target.value)}
                    placeholder="./storage-local"
                    className={`mt-1 ${inputClass}`}
                  />
                </>
              ) : (
                <>
                  <FieldLabel>{t('systemConfig.backendCredentials')}</FieldLabel>
                  <BackendConfigHint backend={config.default_backend} />
                </>
              )}
            </div>
          </div>
        </div>
      </div>

      <div className={cardClass}>
        <CardHeader icon={KeyRound} title={t('systemConfig.security')} />
        <div className="grid gap-3 sm:grid-cols-2">
          <div>
            <FieldLabel>{t('systemConfig.jwtSecret')}</FieldLabel>
            <input
              type="password"
              value={config.jwt_secret}
              readOnly
              disabled
              className={`mt-1 ${inputClass}`}
            />
          </div>
          <div>
            <FieldLabel>{t('systemConfig.tokenEncryptionKey')}</FieldLabel>
            <input
              type="password"
              value={config.token_encryption_key}
              readOnly
              disabled
              className={`mt-1 ${inputClass}`}
            />
          </div>
        </div>
        <p className="mt-3 text-xs" style={{ color: 'var(--color-text-muted)' }}>
          {t('systemConfig.secretsNote')}
        </p>
      </div>

      <div className={cardClass}>
        <CardHeader icon={Archive} title={t('systemConfig.backups')} />
        <div className="mb-3 flex items-center justify-between">
          <p className="text-xs" style={{ color: 'var(--color-text-muted)' }}>
            {t('systemConfig.backupCount', { count: backups.length })}
          </p>
          <Button variant="ghost" size="sm" onClick={() => void handleBackup()} disabled={backingUp}>
            {backingUp ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Archive className="h-3.5 w-3.5" />}
            {t('systemConfig.backupCurrent')}
          </Button>
        </div>
        {backups.length === 0 ? (
          <p className="text-sm" style={{ color: 'var(--color-text-muted)' }}>{t('systemConfig.noBackups')}</p>
        ) : (
          <ul className="divide-y" style={{ borderColor: 'var(--color-border)' }}>
            {backups.map(b => (
              <li key={b.filename} className="flex items-center justify-between py-2">
                <span className="font-mono text-xs" style={{ color: 'var(--color-text-secondary)' }}>
                  {b.filename}
                </span>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => void handleRestore(b.filename)}
                  disabled={restoring !== null}
                >
                  {restoring === b.filename ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RotateCcw className="h-3.5 w-3.5" />}
                  {t('systemConfig.restore')}
                </Button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="flex items-center justify-between rounded-lg border border-[var(--color-border)] bg-[var(--glass-tint-base)]/65 p-4 backdrop-blur-sm">
        <p className="text-xs" style={{ color: 'var(--color-text-muted)' }}>
          {changedCount > 0
            ? t('systemConfig.changedCount', { count: changedCount })
            : t('systemConfig.noUnsavedChanges')}
        </p>
        <Button onClick={() => void handleSave()} disabled={saving || changedCount === 0}>
          {saving ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Save className="h-3.5 w-3.5" />}
          {t('systemConfig.saveRestart')}
        </Button>
      </div>

      <p className="text-xs" style={{ color: 'var(--color-text-muted)' }}>
        {t('systemConfig.configFile')} <span className="font-mono">{config.config_path}</span>
      </p>
    </div>
  )
}
