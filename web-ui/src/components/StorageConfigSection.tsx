import { useState, type FormEvent } from 'react'
import { createPortal } from 'react-dom'
import { useTranslation } from 'react-i18next'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import {
  FolderGit,
  Server,
  Trash2,
  Pencil,
  Plus,
  Loader2,
  Eye,
  EyeOff,
  Check,
  Star,
  X,
} from 'lucide-react'
import {
  listStorageConfigs,
  createStorageConfig,
  updateStorageConfig,
  deleteStorageConfig,
  setDefaultStorageConfig,
  type UserStorageConfig,
  type CreateStorageConfigRequest,
} from '../api/client'
import GlassSelect from './ui/GlassSelect'

// ── Helpers ────────────────────────────────────────────────
const PROVIDER_OPTIONS = [
  { value: 'github', label: 'GitHub', Icon: FolderGit },
  { value: 'gitcode', label: 'GitCode', Icon: Server },
] as const

type Provider = (typeof PROVIDER_OPTIONS)[number]['value']

interface FormState {
  name: string
  provider: Provider
  token: string
  repo: string
  branch: string
  path_prefix: string
  is_default: boolean
}

const EMPTY_FORM: FormState = {
  name: '',
  provider: 'github',
  token: '',
  repo: '',
  branch: 'main',
  path_prefix: '',
  is_default: false,
}

function providerIcon(p: string) {
  return p === 'github' ? FolderGit : Server
}

// ── Modal Component ────────────────────────────────────────
interface ConfigModalProps {
  editing: UserStorageConfig | null
  onClose: () => void
}

function ConfigModal({ editing, onClose }: ConfigModalProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [form, setForm] = useState<FormState>(
    editing
      ? {
          name: editing.name,
          provider: editing.provider as Provider,
          token: '',
          repo: editing.repo,
          branch: editing.branch,
          path_prefix: editing.path_prefix ?? '',
          is_default: editing.is_default,
        }
      : { ...EMPTY_FORM },
  )
  const [showToken, setShowToken] = useState(false)
  const [saving, setSaving] = useState(false)
  const isEdit = !!editing

  function set<K extends keyof FormState>(key: K, value: FormState[K]) {
    setForm((prev) => ({ ...prev, [key]: value }))
  }

  function validate(): string | null {
    if (!form.name.trim()) return t('storageConfig.nameRequired')
    if (!isEdit && !form.token.trim()) return t('storageConfig.tokenRequired')
    if (!form.repo.trim()) return t('storageConfig.repoRequired')
    if (!form.branch.trim()) return t('storageConfig.branchRequired')
    if (!form.repo.includes('/')) return t('storageConfig.repoFormat')
    return null
  }

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    const err = validate()
    if (err) {
      toast.error(err)
      return
    }
    setSaving(true)
    try {
      if (isEdit) {
        const data: Record<string, string> = {
          name: form.name.trim(),
          repo: form.repo.trim(),
          branch: form.branch.trim(),
          path_prefix: form.path_prefix.trim() || '',
        }
        if (form.token.trim()) data.token = form.token.trim()
        await updateStorageConfig(editing.id, data)
        toast.success(t('storageConfig.configUpdated'))
      } else {
        const data: CreateStorageConfigRequest = {
          name: form.name.trim(),
          provider: form.provider,
          token: form.token.trim(),
          repo: form.repo.trim(),
          branch: form.branch.trim(),
          path_prefix: form.path_prefix.trim() || undefined,
          is_default: form.is_default,
        }
        await createStorageConfig(data)
        toast.success(t('storageConfig.configCreated'))
      }
      queryClient.invalidateQueries({ queryKey: ['storage-configs'] })
      onClose()
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('storageConfig.saveFailed'))
    } finally {
      setSaving(false)
    }
  }

  const fieldStyle: React.CSSProperties = {
    background: 'color-mix(in oklch, var(--glass-tint-base) 30%, transparent)',
    border: '1px solid var(--glass-border-base)',
    color: 'var(--color-text-primary)',
  }

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" onClick={onClose} />

      <div
        className="relative w-full max-w-md rounded-xl p-6"
        style={{
          backgroundColor: 'color-mix(in oklch, var(--glass-tint-base) calc(var(--glass-layer-modal-opacity) * 100%), transparent)',
          border: '1px solid var(--glass-border-base)',
          borderTopColor: 'var(--glass-border-strong)',
          backdropFilter: 'blur(var(--glass-layer-modal-blur)) saturate(var(--glass-layer-modal-saturate))',
          boxShadow: 'var(--glass-shadow)',
        }}
      >
        {/* Header */}
        <div className="mb-5 flex items-center justify-between">
          <h2
            className="text-lg font-semibold"
            style={{ color: 'var(--color-text-primary)' }}
          >
            {isEdit ? t('storageConfig.editTitle') : t('storageConfig.addTitle')}
          </h2>
          <button
            onClick={onClose}
            className="rounded p-1 transition-colors hover:bg-[var(--color-surface)]"
            style={{ color: 'var(--color-text-muted)' }}
          >
            <X className="h-5 w-5" />
          </button>
        </div>

        <form onSubmit={handleSubmit} className="space-y-4">
          {/* Name */}
          <div>
            <label
              className="mb-1 block text-sm font-medium"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              {t('storageConfig.name')}
            </label>
            <input
              type="text"
              required
              placeholder={form.provider === 'github' ? t('storageConfig.namePlaceholderGithub') : t('storageConfig.namePlaceholderGitcode')}
              value={form.name}
              onChange={(e) => set('name', e.target.value)}
              className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
              style={fieldStyle}
            />
          </div>

          {/* Provider */}
          <div>
            <label
              className="mb-1 block text-sm font-medium"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              {t('storageConfig.provider')}
            </label>
            <GlassSelect
              value={form.provider}
              onChange={(v) => set('provider', v as Provider)}
              options={PROVIDER_OPTIONS}
              disabled={isEdit}
              className="block w-full"
            />
          </div>

          {/* Token */}
          <div>
            <label
              className="mb-1 block text-sm font-medium"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              {t('storageConfig.token')} {isEdit && t('storageConfig.tokenKeepHint')}
            </label>
            <div className="relative">
              <input
                type={showToken ? 'text' : 'password'}
                required={!isEdit}
                placeholder={isEdit ? t('storageConfig.tokenPlaceholderEdit') : t('storageConfig.tokenPlaceholderNew')}
                value={form.token}
                onChange={(e) => set('token', e.target.value)}
                className="block w-full rounded-lg py-2 pl-3 pr-10 text-sm focus:outline-none focus:ring-1"
                style={fieldStyle}
              />
              <button
                type="button"
                onClick={() => setShowToken(!showToken)}
                className="absolute right-2 top-1/2 -translate-y-1/2 rounded p-1 transition-colors hover:bg-[var(--color-surface)]"
                style={{ color: 'var(--color-text-muted)' }}
                tabIndex={-1}
              >
                {showToken ? <EyeOff className="h-4 w-4" /> : <Eye className="h-4 w-4" />}
              </button>
            </div>
          </div>

          {/* Repo */}
          <div>
            <label
              className="mb-1 block text-sm font-medium"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              {t('storageConfig.repo')}
            </label>
            <input
              type="text"
              required
              placeholder={t('storageConfig.repoPlaceholder')}
              value={form.repo}
              onChange={(e) => set('repo', e.target.value)}
              className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
              style={fieldStyle}
            />
          </div>

          {/* Branch + Path Prefix */}
          <div className="grid grid-cols-2 gap-3">
            <div>
              <label
                className="mb-1 block text-sm font-medium"
                style={{ color: 'var(--color-text-secondary)' }}
              >
                {t('storageConfig.branch')}
              </label>
              <input
                type="text"
                required
                value={form.branch}
                onChange={(e) => set('branch', e.target.value)}
                className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
                style={fieldStyle}
              />
            </div>
            <div>
              <label
                className="mb-1 block text-sm font-medium"
                style={{ color: 'var(--color-text-secondary)' }}
              >
                {t('storageConfig.pathPrefix')}
              </label>
              <input
                type="text"
                placeholder={t('storageConfig.pathPrefixPlaceholder')}
                value={form.path_prefix}
                onChange={(e) => set('path_prefix', e.target.value)}
                className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
                style={fieldStyle}
              />
            </div>
          </div>

          {/* Default Checkbox */}
          <label className="flex items-center gap-2">
            <input
              type="checkbox"
              checked={form.is_default}
              onChange={(e) => set('is_default', e.target.checked)}
              className="rounded"
            />
            <span className="text-sm" style={{ color: 'var(--color-text-secondary)' }}>
              {t('storageConfig.setAsDefault')}
            </span>
          </label>

          {/* Actions */}
          <div className="flex items-center justify-between pt-2">
            <div />
            <div className="flex gap-3">
              <button
                type="button"
                onClick={onClose}
                className="rounded-lg px-4 py-2 text-sm transition-colors"
                style={{ color: 'var(--color-text-muted)' }}
              >
                {t('storageConfig.cancel')}
              </button>
              <button
                type="submit"
                disabled={saving}
                className="rounded-lg px-4 py-2 text-sm font-medium text-white disabled:opacity-50"
                style={{ backgroundColor: 'var(--color-accent)' }}
              >
                {saving ? (
                  <>
                    <Loader2 className="mr-1 inline h-3.5 w-3.5 animate-spin" />
                    {t('storageConfig.saving')}
                  </>
                ) : isEdit ? (
                  t('storageConfig.update')
                ) : (
                  t('storageConfig.create')
                )}
              </button>
            </div>
          </div>
        </form>
      </div>
    </div>,
    document.body,
  )
}

// ── Delete Confirm ─────────────────────────────────────────
interface DeleteConfirmProps {
  config: UserStorageConfig
  onClose: () => void
}

function DeleteConfirm({ config, onClose }: DeleteConfirmProps) {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const [deleting, setDeleting] = useState(false)

  async function handleDelete() {
    setDeleting(true)
    try {
      await deleteStorageConfig(config.id)
      toast.success(t('storageConfig.configDeleted'))
      queryClient.invalidateQueries({ queryKey: ['storage-configs'] })
      onClose()
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('storageConfig.deleteFailed'))
    } finally {
      setDeleting(false)
    }
  }

  return createPortal(
    <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
      <div className="absolute inset-0 bg-black/50 backdrop-blur-sm" onClick={onClose} />

      <div
        className="relative w-full max-w-sm rounded-xl p-6"
        style={{
          backgroundColor: 'color-mix(in oklch, var(--glass-tint-base) calc(var(--glass-layer-modal-opacity) * 100%), transparent)',
          border: '1px solid var(--glass-border-base)',
          borderTopColor: 'var(--glass-border-strong)',
          backdropFilter: 'blur(var(--glass-layer-modal-blur)) saturate(var(--glass-layer-modal-saturate))',
          boxShadow: 'var(--glass-shadow)',
        }}
      >
        <h2
          className="mb-2 text-lg font-semibold"
          style={{ color: 'var(--color-text-primary)' }}
        >
          {t('storageConfig.deleteConfigTitle')}
        </h2>
        <p className="mb-1 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
          {t('storageConfig.deleteConfirmPrefix')}{' '}
          <strong>{config.name}</strong>
          {t('storageConfig.deleteConfirmSuffix')}
        </p>
        <p className="mb-4 text-xs" style={{ color: 'var(--color-danger)' }}>
          {t('storageConfig.deleteWarning')}
        </p>

        <div className="flex justify-end gap-3">
          <button
            type="button"
            onClick={onClose}
            disabled={deleting}
            className="rounded-lg px-4 py-2 text-sm transition-colors"
            style={{ color: 'var(--color-text-muted)' }}
          >
            {t('storageConfig.cancel')}
          </button>
          <button
            type="button"
            onClick={handleDelete}
            disabled={deleting}
            className="rounded-lg px-4 py-2 text-sm font-medium text-white disabled:opacity-50"
            style={{ backgroundColor: 'var(--color-danger)' }}
          >
            {deleting ? (
              <>
                <Loader2 className="mr-1 inline h-3.5 w-3.5 animate-spin" />
                {t('storageConfig.deleting')}
              </>
            ) : (
              t('storageConfig.delete')
            )}
          </button>
        </div>
      </div>
    </div>,
    document.body,
  )
}

// ── Main Section ───────────────────────────────────────────
export default function StorageConfigSection() {
  const { t } = useTranslation()
  const queryClient = useQueryClient()
  const { data: configs, isLoading, error } = useQuery({
    queryKey: ['storage-configs'],
    queryFn: listStorageConfigs,
  })

  const [modalOpen, setModalOpen] = useState(false)
  const [editingConfig, setEditingConfig] = useState<UserStorageConfig | null>(null)
  const [deletingConfig, setDeletingConfig] = useState<UserStorageConfig | null>(null)

  function openAdd() {
    setEditingConfig(null)
    setModalOpen(true)
  }

  function openEdit(config: UserStorageConfig) {
    setEditingConfig(config)
    setModalOpen(true)
  }

  function closeModal() {
    setModalOpen(false)
    setEditingConfig(null)
  }

  async function handleSetDefault(id: string) {
    try {
      await setDefaultStorageConfig(id)
      toast.success(t('storageConfig.defaultUpdated'))
      queryClient.invalidateQueries({ queryKey: ['storage-configs'] })
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('storageConfig.setDefaultFailed'))
    }
  }

  return (
    <div
      className="space-y-3 rounded-lg border border-[var(--color-border)] bg-[var(--glass-tint-base)]/65 p-4 backdrop-blur-sm"
    >
      {/* Header */}
      <div className="flex items-center justify-between">
        <h3
          className="text-sm font-medium"
          style={{ color: 'var(--color-text-primary)' }}
        >
          {t('storageConfig.title')}
        </h3>
        <button
          onClick={openAdd}
          className="flex items-center gap-1 rounded-lg px-3 py-1.5 text-xs font-medium text-white transition-colors"
          style={{ backgroundColor: 'var(--color-accent)' }}
        >
          <Plus className="h-3.5 w-3.5" />
          {t('storageConfig.addButton')}
        </button>
      </div>

      {/* Loading */}
      {isLoading && (
        <div className="flex items-center justify-center py-6">
          <Loader2
            className="h-5 w-5 animate-spin"
            style={{ color: 'var(--color-text-muted)' }}
          />
        </div>
      )}

      {/* Error */}
      {error && (
        <p
          className="py-3 text-center text-sm"
          style={{ color: 'var(--color-danger)' }}
        >
          {t('storageConfig.failedToLoad')}
        </p>
      )}

      {/* Empty */}
      {!isLoading && !error && configs && configs.length === 0 && (
        <p
          className="py-6 text-center text-sm"
          style={{ color: 'var(--color-text-muted)' }}
        >
          {t('storageConfig.emptyState')}
        </p>
      )}

      {/* Config Cards */}
      {!isLoading && configs && configs.length > 0 && (
        <div className="space-y-2">
          {configs.map((cfg) => {
            const PIcon = providerIcon(cfg.provider)
            return (
              <div
                key={cfg.id}
                className="flex items-start gap-3 rounded-lg border p-3 transition-colors"
                style={{
                  backgroundColor: cfg.is_default
                    ? 'var(--color-accent-subtle)'
                    : 'var(--color-surface)',
                  borderColor: cfg.is_default
                    ? 'var(--color-accent)'
                    : 'var(--color-border)',
                }}
              >
                {/* Provider Icon */}
                <div
                  className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center rounded-lg"
                  style={{ backgroundColor: 'var(--color-surface-glass)' }}
                >
                  <PIcon
                    className="h-4 w-4"
                    style={{ color: 'var(--color-text-secondary)' }}
                  />
                </div>

                {/* Info */}
                <div className="min-w-0 flex-1">
                  <div className="flex items-center gap-2">
                    <span
                      className="text-sm font-medium truncate"
                      style={{ color: 'var(--color-text-primary)' }}
                    >
                      {cfg.name}
                    </span>
                    {cfg.is_default && (
                      <span
                        className="inline-flex items-center gap-0.5 rounded-full px-2 py-0.5 text-[10px] font-medium"
                        style={{
                          backgroundColor: 'var(--color-accent)',
                          color: 'white',
                        }}
                      >
                        <Star className="h-2.5 w-2.5" />
                        {t('storageConfig.defaultBadge')}
                      </span>
                    )}
                  </div>
                  <p
                    className="mt-0.5 truncate text-xs"
                    style={{ color: 'var(--color-text-muted)' }}
                  >
                    {cfg.repo}
                    {cfg.path_prefix ? ` / ${cfg.path_prefix}` : ''}{' '}
                    @ {cfg.branch}
                  </p>
                </div>

                {/* Actions */}
                <div className="flex shrink-0 items-center gap-0.5">
                  {!cfg.is_default && (
                    <button
                      onClick={() => handleSetDefault(cfg.id)}
                      className="rounded p-1.5 transition-colors hover:bg-[var(--color-surface)]"
                      style={{ color: 'var(--color-text-muted)' }}
                      title={t('storageConfig.setDefaultTitle')}
                    >
                      <Check className="h-3.5 w-3.5" />
                    </button>
                  )}
                  <button
                    onClick={() => openEdit(cfg)}
                    className="rounded p-1.5 transition-colors hover:bg-[var(--color-surface)]"
                    style={{ color: 'var(--color-text-muted)' }}
                    title={t('storageConfig.editAria')}
                  >
                    <Pencil className="h-3.5 w-3.5" />
                  </button>
                  <button
                    onClick={() => setDeletingConfig(cfg)}
                    className="rounded p-1.5 transition-colors hover:bg-[var(--color-surface)]"
                    style={{ color: 'var(--color-text-muted)' }}
                    title={t('storageConfig.deleteAria')}
                  >
                    <Trash2 className="h-3.5 w-3.5" />
                  </button>
                </div>
              </div>
            )
          })}
        </div>
      )}

      {/* Modals */}
      {modalOpen && <ConfigModal editing={editingConfig} onClose={closeModal} />}
      {deletingConfig && (
        <DeleteConfirm
          config={deletingConfig}
          onClose={() => setDeletingConfig(null)}
        />
      )}
    </div>
  )
}
