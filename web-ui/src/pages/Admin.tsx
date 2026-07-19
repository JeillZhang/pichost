import { useState } from 'react'
import AdminStats from './admin/AdminStats'
import AdminUsers from './admin/AdminUsers'
import AdminInvites from './admin/AdminInvites'

type Tab = 'overview' | 'users' | 'invites'

export default function Admin() {
  const [activeTab, setActiveTab] = useState<Tab>('overview')

  return (
    <div>
      <h1 className="mb-4 text-lg font-bold" style={{ color: 'var(--color-text-primary)' }}>
        Admin Panel
      </h1>

      <div
        className="mb-4 flex gap-1 rounded-xl p-1"
        style={{
          backgroundColor: 'var(--color-surface)',
          border: '1px solid var(--color-border)',
        }}
      >
        <button
          onClick={() => setActiveTab('overview')}
          className="flex-1 rounded-lg px-4 py-2 text-sm font-medium transition-colors"
          style={{
            backgroundColor: activeTab === 'overview' ? 'var(--color-accent-subtle)' : 'transparent',
            color: activeTab === 'overview' ? 'var(--color-accent)' : 'var(--color-text-muted)',
          }}
        >
          Overview
        </button>
        <button
          onClick={() => setActiveTab('users')}
          className="flex-1 rounded-lg px-4 py-2 text-sm font-medium transition-colors"
          style={{
            backgroundColor: activeTab === 'users' ? 'var(--color-accent-subtle)' : 'transparent',
            color: activeTab === 'users' ? 'var(--color-accent)' : 'var(--color-text-muted)',
          }}
        >
          Users
        </button>
        <button
          onClick={() => setActiveTab('invites')}
          className="flex-1 rounded-lg px-4 py-2 text-sm font-medium transition-colors"
          style={{
            backgroundColor: activeTab === 'invites' ? 'var(--color-accent-subtle)' : 'transparent',
            color: activeTab === 'invites' ? 'var(--color-accent)' : 'var(--color-text-muted)',
          }}
        >
          Invites
        </button>
      </div>

      {activeTab === 'overview' && <AdminStats />}
      {activeTab === 'users' && <AdminUsers />}
      {activeTab === 'invites' && <AdminInvites />}
    </div>
  )
}
