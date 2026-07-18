import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { Plus, Copy, Clock } from 'lucide-react'
import { toast } from 'sonner'
import { listInviteCodes, type InviteCodeInfo } from '../api/client'
import CreateInviteDialog from '../components/CreateInviteDialog'

function formatDate(timestamp: number): string {
  return new Date(timestamp * 1000).toLocaleDateString('en-US', {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  })
}

function timeRemaining(expiresAt: number): string {
  const now = Math.floor(Date.now() / 1000)
  const diff = expiresAt - now
  if (diff <= 0) return 'Expired'
  const days = Math.floor(diff / 86400)
  const hours = Math.floor((diff % 86400) / 3600)
  if (days > 0) return `${days}d ${hours}h remaining`
  return `${hours}h remaining`
}

function getStatus(code: InviteCodeInfo): { label: string; color: string } {
  if (code.used_by) {
    return { label: 'Used', color: 'var(--color-text-muted)' }
  }
  const now = Math.floor(Date.now() / 1000)
  if (code.expires_at <= now) {
    return { label: 'Expired', color: 'var(--color-danger)' }
  }
  return { label: 'Active', color: 'var(--color-success)' }
}

function truncateCode(code: string, maxLen = 16): string {
  if (code.length <= maxLen) return code
  return `${code.slice(0, 8)}…${code.slice(-6)}`
}

export default function AdminInvites() {
  const [showCreate, setShowCreate] = useState(false)
  const queryClient = useQueryClient()

  const { data: codes, isLoading } = useQuery({
    queryKey: ['admin', 'invites'],
    queryFn: listInviteCodes,
    refetchInterval: 30_000,
  })

  async function handleCopy(code: string) {
    try {
      await navigator.clipboard.writeText(code)
      toast.success('Copied to clipboard')
    } catch {
      toast.error('Failed to copy')
    }
  }

  function handleCreated() {
    setShowCreate(false)
    queryClient.invalidateQueries({ queryKey: ['admin', 'invites'] })
  }

  if (isLoading) {
    return (
      <div className="flex items-center justify-center py-20" style={{ color: 'var(--color-text-muted)' }}>
        Loading invite codes…
      </div>
    )
  }

  if (!codes || codes.length === 0) {
    return (
      <div>
        <div className="mb-3 flex items-center justify-between">
          <p className="text-sm" style={{ color: 'var(--color-text-muted)' }}>
            0 codes
          </p>
          <button
            onClick={() => setShowCreate(true)}
            className="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium text-white transition-colors"
            style={{ backgroundColor: 'var(--color-accent)' }}
          >
            <Plus className="h-4 w-4" />
            Create Code
          </button>
        </div>

        <div
          className="flex flex-col items-center justify-center rounded-xl py-16"
          style={{
            backgroundColor: 'var(--glass-bg)',
            border: '1px solid var(--glass-border)',
          }}
        >
          <Clock className="mb-3 h-8 w-8" style={{ color: 'var(--color-text-muted)' }} />
          <p className="mb-1 text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
            No active invite codes
          </p>
          <p className="text-sm" style={{ color: 'var(--color-text-muted)' }}>
            <button
              onClick={() => setShowCreate(true)}
              className="font-medium underline transition-colors"
              style={{ color: 'var(--color-accent)' }}
            >
              Create one
            </button>{' '}
            to let others join
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
          {codes.length} code{codes.length !== 1 ? 's' : ''}
        </p>
        <button
          onClick={() => setShowCreate(true)}
          className="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm font-medium text-white transition-colors"
          style={{ backgroundColor: 'var(--color-accent)' }}
        >
          <Plus className="h-4 w-4" />
          Create Code
        </button>
      </div>

      <div
        className="overflow-hidden rounded-xl"
        style={{
          backgroundColor: 'var(--glass-bg)',
          border: '1px solid var(--glass-border)',
        }}
      >
        <table className="w-full text-sm">
          <thead>
            <tr style={{ borderBottom: '1px solid var(--color-border)' }}>
              <th className="px-4 py-3 text-left font-medium" style={{ color: 'var(--color-text-muted)' }}>Code</th>
              <th className="hidden px-4 py-3 text-left font-medium sm:table-cell" style={{ color: 'var(--color-text-muted)' }}>Created</th>
              <th className="hidden px-4 py-3 text-left font-medium md:table-cell" style={{ color: 'var(--color-text-muted)' }}>Expires</th>
              <th className="px-4 py-3 text-center font-medium" style={{ color: 'var(--color-text-muted)' }}>Status</th>
              <th className="px-4 py-3 text-right font-medium" style={{ color: 'var(--color-text-muted)' }}>Actions</th>
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
                    {formatDate(code.created_at)}
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
                        backgroundColor: status.label === 'Active'
                          ? 'rgba(34, 197, 94, 0.1)'
                          : status.label === 'Expired'
                            ? 'rgba(239, 68, 68, 0.1)'
                            : 'rgba(156, 163, 175, 0.1)',
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
