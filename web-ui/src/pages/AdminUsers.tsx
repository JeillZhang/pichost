import { useState } from 'react'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { toast } from 'sonner'
import { Trash2, Pencil } from 'lucide-react'
import api, { type UserInfo } from '../api/client'
import EditUserDialog from '../components/EditUserDialog'

interface ListUsersResponse {
  users: UserInfo[]
  total: number
}

export default function AdminUsers() {
  const [editingUser, setEditingUser] = useState<UserInfo | null>(null)
  const queryClient = useQueryClient()

  const { data, isLoading } = useQuery({
    queryKey: ['admin', 'users'],
    queryFn: () => api.get('admin/users?offset=0&limit=50').json<ListUsersResponse>(),
  })

  async function handleDelete(user: UserInfo) {
    if (!confirm(`Delete user "${user.username}"? This will permanently delete all their images.`)) return
    try {
      await api.delete(`admin/users/${user.id}`).json()
      toast.success(`User "${user.username}" deleted`)
      queryClient.invalidateQueries({ queryKey: ['admin', 'users'] })
      queryClient.invalidateQueries({ queryKey: ['admin', 'stats'] })
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Delete failed'
      toast.error(msg)
    }
  }

  if (isLoading || !data) {
    return (
      <div className="flex items-center justify-center py-20" style={{ color: 'var(--color-text-muted)' }}>
        Loading users…
      </div>
    )
  }

  return (
    <div>
      <div className="mb-3 flex items-center justify-between">
        <p className="text-sm" style={{ color: 'var(--color-text-muted)' }}>
          {data.total} user{data.total !== 1 ? 's' : ''} total
        </p>
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
              <th className="px-4 py-3 text-left font-medium" style={{ color: 'var(--color-text-muted)' }}>Username</th>
              <th className="hidden px-4 py-3 text-left font-medium sm:table-cell" style={{ color: 'var(--color-text-muted)' }}>Email</th>
              <th className="px-4 py-3 text-center font-medium" style={{ color: 'var(--color-text-muted)' }}>Admin</th>
              <th className="px-4 py-3 text-right font-medium" style={{ color: 'var(--color-text-muted)' }}>Actions</th>
            </tr>
          </thead>
          <tbody>
            {data.users.map((user) => (
              <tr
                key={user.id}
                style={{ borderBottom: '1px solid var(--color-border)' }}
                className="hover:opacity-80"
              >
                <td className="px-4 py-3" style={{ color: 'var(--color-text-primary)' }}>
                  {user.username}
                </td>
                <td className="hidden px-4 py-3 sm:table-cell" style={{ color: 'var(--color-text-secondary)' }}>
                  {user.email || '—'}
                </td>
                <td className="px-4 py-3 text-center">
                  {user.is_admin ? (
                    <span
                      className="inline-block rounded px-2 py-0.5 text-xs font-medium"
                      style={{ backgroundColor: 'rgba(59, 130, 246, 0.1)', color: '#3b82f6' }}
                    >
                      Admin
                    </span>
                  ) : (
                    <span style={{ color: 'var(--color-text-muted)' }}>—</span>
                  )}
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
                      onClick={() => handleDelete(user)}
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
    </div>
  )
}
