import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { Trash2, Pencil } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useFormat } from '../../hooks/useFormat'
import api, { type UserInfo } from '../../api/client'
import EditUserDialog from '../../components/EditUserDialog'
import ConfirmDialog from '../../components/ui/ConfirmDialog'

interface ListUsersResponse {
  users: UserInfo[]
  total: number
}

export default function AdminUsers() {
  const { t } = useTranslation()
  const { formatBytes } = useFormat()
  const [editingUser, setEditingUser] = useState<UserInfo | null>(null)
  const [deleteUser, setDeleteUser] = useState<UserInfo | null>(null)
  const queryClient = useQueryClient()

  const { data, isLoading } = useQuery({
    queryKey: ['admin', 'users'],
    queryFn: () => api.get('admin/users?offset=0&limit=50').json<ListUsersResponse>(),
  })

  async function handleDelete(user: UserInfo) {
    try {
      await api.delete(`admin/users/${user.id}`).json()
      toast.success(t('adminUsers.deleted', { name: user.username }))
      queryClient.invalidateQueries({ queryKey: ['admin', 'users'] })
      queryClient.invalidateQueries({ queryKey: ['admin', 'stats'] })
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : t('adminUsers.deleteFailed')
      toast.error(msg)
    }
  }

  if (isLoading || !data) {
    return (
      <div className="flex items-center justify-center py-20" style={{ color: 'var(--color-text-muted)' }}>
        {t('adminUsers.loading')}
      </div>
    )
  }

  return (
    <div>
      <div className="mb-3 flex items-center justify-between">
        <p className="text-sm" style={{ color: 'var(--color-text-muted)' }}>
          {t('adminUsers.totalCount', { count: data.total })}
        </p>
      </div>

      <div className="mt-3 flex flex-col gap-2 sm:hidden">
        {data.users.map((user) => (
          <div key={user.id} data-testid="user-card" className="glass rounded-xl p-4">
            <div className="flex items-center justify-between gap-2">
              <span className="truncate text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
                {user.username}
              </span>
              {user.is_admin && (
                <span
                  className="badge shrink-0"
                  style={{ backgroundColor: 'var(--color-accent-subtle)', color: 'var(--color-accent)', borderColor: 'var(--color-accent-strong)' }}
                >
                  {t('adminUsers.adminBadge')}
                </span>
              )}
            </div>
            <div className="mt-1 truncate text-xs" style={{ color: 'var(--color-text-secondary)' }}>
              {user.email || '—'}
            </div>
            <div className="mt-1 font-mono text-xs" style={{ color: 'var(--color-text-secondary)' }}>
              {user.storage_quota != null ? formatBytes(user.storage_quota) : t('adminUsers.unlimited')}
            </div>
            <div className="mt-3 flex justify-end gap-2">
              <button
                onClick={() => setEditingUser(user)}
                className="flex min-h-[44px] items-center gap-1.5 rounded-lg px-3 text-sm"
                style={{ color: 'var(--color-text-secondary)' }}
                aria-label={t('editUser.title')}
              >
                <Pencil className="h-4 w-4" />
              </button>
              <button
                onClick={() => setDeleteUser(user)}
                className="flex min-h-[44px] items-center gap-1.5 rounded-lg px-3 text-sm"
                style={{ color: 'var(--color-danger)' }}
              >
                <Trash2 className="h-4 w-4" /> {t('common.delete')}
              </button>
            </div>
          </div>
        ))}
      </div>

      <div className="glass hidden overflow-x-auto rounded-xl sm:block">
        <table className="w-full text-sm">
          <thead>
            <tr style={{ borderBottom: '1px solid var(--color-border)' }}>
              <th className="px-4 py-3 text-left font-medium" style={{ color: 'var(--color-text-muted)' }}>{t('adminUsers.username')}</th>
              <th className="hidden px-4 py-3 text-left font-medium sm:table-cell" style={{ color: 'var(--color-text-muted)' }}>{t('adminUsers.email')}</th>
              <th className="px-4 py-3 text-center font-medium" style={{ color: 'var(--color-text-muted)' }}>{t('adminUsers.admin')}</th>
              <th className="hidden px-4 py-3 text-right font-medium sm:table-cell" style={{ color: 'var(--color-text-muted)' }}>{t('adminUsers.quota')}</th>
              <th className="px-4 py-3 text-right font-medium" style={{ color: 'var(--color-text-muted)' }}>{t('adminUsers.actions')}</th>
            </tr>
          </thead>
          <tbody>
            {data.users.map((user) => (
              <tr
                key={user.id}
                style={{ borderBottom: '1px solid var(--color-border)' }}
                className="transition-colors duration-100 hover:bg-[var(--color-surface)]"
              >
                <td className="truncate max-w-[180px] px-4 py-3" style={{ color: 'var(--color-text-primary)' }}>
                  {user.username}
                </td>
                <td className="hidden px-4 py-3 sm:table-cell" style={{ color: 'var(--color-text-secondary)' }}>
                  {user.email || '—'}
                </td>
                <td className="px-4 py-3 text-center">
                  {user.is_admin ? (
                    <span
                      className="badge" style={{ backgroundColor: 'var(--color-accent-subtle)', color: 'var(--color-accent)', borderColor: 'var(--color-accent-strong)' }}
                    >
                      {t('adminUsers.adminBadge')}
                    </span>
                  ) : (
                    <span style={{ color: 'var(--color-text-muted)' }}>—</span>
                  )}
                </td>
                <td className="hidden px-4 py-3 text-right font-mono text-xs sm:table-cell" style={{ color: 'var(--color-text-secondary)' }}>
                  {user.storage_quota != null ? formatBytes(user.storage_quota) : t('adminUsers.unlimited')}
                </td>
                <td className="px-4 py-3 text-right">
                  <div className="flex items-center justify-end gap-2">
                    <button
                      onClick={() => setEditingUser(user)}
                      className="rounded p-1.5 transition-colors"
                      style={{ color: 'var(--color-text-muted)' }}
                      onMouseEnter={(e) => { e.currentTarget.style.backgroundColor = 'var(--color-surface)'; e.currentTarget.style.color = 'var(--color-text-secondary)' }}
                      onMouseLeave={(e) => { e.currentTarget.style.backgroundColor = 'transparent'; e.currentTarget.style.color = 'var(--color-text-muted)' }}
                    >
                      <Pencil className="h-3.5 w-3.5" />
                    </button>
                    <button
                      onClick={() => setDeleteUser(user)}
                      className="rounded p-1.5 transition-colors"
                      style={{ color: 'var(--color-text-muted)' }}
                      onMouseEnter={(e) => { e.currentTarget.style.backgroundColor = 'var(--color-danger-subtle)'; e.currentTarget.style.color = 'var(--color-danger)' }}
                      onMouseLeave={(e) => { e.currentTarget.style.backgroundColor = 'transparent'; e.currentTarget.style.color = 'var(--color-text-muted)' }}
                    >
                      <Trash2 className="h-3.5 w-3.5" />
                    </button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {editingUser && (
        <EditUserDialog
          user={editingUser}
          onClose={() => setEditingUser(null)}
          onUpdated={() => {
            setEditingUser(null)
            queryClient.invalidateQueries({ queryKey: ['admin', 'users'] })
          }}
        />
      )}

      <ConfirmDialog
        open={!!deleteUser}
        onClose={() => setDeleteUser(null)}
        onConfirm={() => deleteUser && handleDelete(deleteUser)}
        title={t('adminUsers.deleteTitle')}
        message={t('adminUsers.deleteConfirm', { name: deleteUser?.username ?? '' })}
        confirmLabel={t('common.delete')}
        danger
      />
    </div>
  )
}
