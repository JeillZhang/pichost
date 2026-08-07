import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Plus, Copy, Clock } from 'lucide-react'
import { toast } from 'sonner'
import { useTranslation } from 'react-i18next'
import { useFormat } from '../../hooks/useFormat'
import { listInviteCodes, type InviteCodeInfo } from '../../api/client'
import CreateInviteDialog from '../../components/CreateInviteDialog'

function truncateCode(code: string, maxLen = 16): string {
  if (code.length <= maxLen) return code
  return `${code.slice(0, 8)}…${code.slice(-6)}`
}

export default function AdminInvites() {
  const { t } = useTranslation()
  const { formatDate } = useFormat()
  const [showCreate, setShowCreate] = useState(false)
  const queryClient = useQueryClient()

  const { data: codes, isLoading } = useQuery({
    queryKey: ['admin', 'invites'],
    queryFn: listInviteCodes,
    refetchInterval: 30_000,
  })

  function timeRemaining(expiresAt: number): string {
    const now = Math.floor(Date.now() / 1000)
    const diff = expiresAt - now
    if (diff <= 0) return t('adminInvites.statusExpired')
    const days = Math.floor(diff / 86400)
    const hours = Math.floor((diff % 86400) / 3600)
    if (days > 0) return t('adminInvites.timeRemaining', { days, hours })
    return t('adminInvites.timeRemainingHours', { hours })
  }

  function getStatus(code: InviteCodeInfo): { key: string; label: string; color: string } {
    if (code.used_by) {
      return { key: 'used', label: t('adminInvites.statusUsed'), color: 'var(--color-text-muted)' }
    }
    const now = Math.floor(Date.now() / 1000)
    if (code.expires_at <= now) {
      return { key: 'expired', label: t('adminInvites.statusExpired'), color: 'var(--color-danger)' }
    }
    return { key: 'active', label: t('adminInvites.statusActive'), color: 'var(--color-success)' }
  }

  async function handleCopy(code: string) {
    try {
      await navigator.clipboard.writeText(code)
      toast.success(t('adminInvites.copySuccess'))
    } catch {
      toast.error(t('adminInvites.copyFailed'))
    }
  }

  function handleCreated() {
    setShowCreate(false)
    queryClient.invalidateQueries({ queryKey: ['admin', 'invites'] })
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-20" style={{ color: 'var(--color-text-muted)' }}>
        {t('adminInvites.loading')}
      </div>
    )
  }

  if (!codes || codes.length === 0) {
    return (
      <div>
        <div className="mb-3 flex items-center justify-between">
          <p className="text-sm" style={{ color: 'var(--color-text-muted)' }}>
            {t('adminInvites.totalCount', { count: codes?.length ?? 0 })}
          </p>
          <button
            onClick={() => setShowCreate(true)}
            className="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium text-white transition-colors"
            style={{ backgroundColor: 'var(--color-accent)' }}
          >
            <Plus className="h-4 w-4" />
            {t('adminInvites.createCode')}
          </button>
        </div>

        <div className="glass flex flex-col items-center justify-center rounded-xl py-16">
          <Clock className="mb-3 h-8 w-8" style={{ color: 'var(--color-text-muted)' }} />
          <p className="mb-1 text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
            {t('adminInvites.emptyState')}
          </p>
          <p className="text-sm" style={{ color: 'var(--color-text-muted)' }}>
            <button
              onClick={() => setShowCreate(true)}
              className="font-medium underline transition-colors"
              style={{ color: 'var(--color-accent)' }}
            >
              {t('adminInvites.createOne')}
            </button>{' '}
            {t('adminInvites.toLetOthersJoin')}
          </p>
        </div>

        {showCreate && (
          <CreateInviteDialog
            onClose={() => setShowCreate(false)}
            onCreated={handleCreated}
          />
        )}
      </div>
    )
  }

  return (
    <div>
      <div className="mb-3 flex items-center justify-between">
        <p className="text-sm" style={{ color: 'var(--color-text-muted)' }}>
          {t('adminInvites.totalCount', { count: codes.length })}
        </p>
        <button
          onClick={() => setShowCreate(true)}
          className="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium text-white transition-colors"
          style={{ backgroundColor: 'var(--color-accent)' }}
        >
          <Plus className="h-4 w-4" />
          {t('adminInvites.createCode')}
        </button>
      </div>

      <div className="glass overflow-hidden rounded-xl">
        <table className="w-full text-sm">
          <thead>
            <tr style={{ borderBottom: '1px solid var(--color-border)' }}>
              <th className="px-4 py-3 text-left font-medium" style={{ color: 'var(--color-text-muted)' }}>{t('adminInvites.tableCode')}</th>
              <th className="hidden px-4 py-3 text-left font-medium sm:table-cell" style={{ color: 'var(--color-text-muted)' }}>{t('adminInvites.tableCreated')}</th>
              <th className="hidden px-4 py-3 text-left font-medium md:table-cell" style={{ color: 'var(--color-text-muted)' }}>{t('adminInvites.tableExpires')}</th>
              <th className="px-4 py-3 text-center font-medium" style={{ color: 'var(--color-text-muted)' }}>{t('adminInvites.tableStatus')}</th>
              <th className="px-4 py-3 text-right font-medium" style={{ color: 'var(--color-text-muted)' }}>{t('adminInvites.tableActions')}</th>
            </tr>
          </thead>
          <tbody>
            {codes.map((code) => {
              const status = getStatus(code)
              return (
                <tr
                  key={code.code}
                  style={{ borderBottom: '1px solid var(--color-border)' }}
                  className="hover:opacity-80"
                >
                  <td className="px-4 py-3 font-mono" style={{ color: 'var(--color-text-primary)' }}>
                    <span title={code.code}>{truncateCode(code.code)}</span>
                  </td>
                  <td className="hidden px-4 py-3 sm:table-cell" style={{ color: 'var(--color-text-secondary)' }}>
                    {formatDate(code.created_at * 1000)}
                  </td>
                  <td className="hidden px-4 py-3 md:table-cell" style={{ color: 'var(--color-text-secondary)' }}>
                    <span className="inline-flex items-center gap-1">
                      <Clock className="h-3 w-3" />
                      {timeRemaining(code.expires_at)}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-center">
                    <span
                      className="inline-block rounded px-2 py-0.5 text-xs font-medium"
                      style={{
                        backgroundColor:
                          status.key === 'active'
                            ? 'var(--color-success-subtle)'
                            : status.key === 'expired'
                              ? 'var(--color-danger-subtle)'
                              : 'var(--color-surface)',
                        color: status.color,
                      }}
                    >
                      {status.label}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-right">
                    <div className="flex items-center justify-end gap-2">
                      <button
                        onClick={() => handleCopy(code.code)}
                        className="rounded p-1.5 transition-colors"
                        style={{ color: 'var(--color-text-muted)' }}
                        onMouseEnter={(e) => { e.currentTarget.style.backgroundColor = 'var(--color-surface)'; e.currentTarget.style.color = 'var(--color-text-secondary)' }}
                        onMouseLeave={(e) => { e.currentTarget.style.backgroundColor = 'transparent'; e.currentTarget.style.color = 'var(--color-text-muted)' }}
                      >
                        <Copy className="h-3.5 w-3.5" />
                      </button>
                    </div>
                  </td>
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>

      {showCreate && (
        <CreateInviteDialog
          onClose={() => setShowCreate(false)}
          onCreated={handleCreated}
        />
      )}
    </div>
  )
}
