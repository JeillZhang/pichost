import { useState } from 'react'
import AdminStats from './admin/AdminStats'
import AdminUsers from './admin/AdminUsers'
import AdminInvites from './admin/AdminInvites'
import SystemConfig from '../components/SystemConfig'

type Tab = 'overview' | 'users' | 'invites' | 'config'

const TABS: { key: Tab; label: string }[] = [
  { key: 'overview', label: 'Overview' },
  { key: 'users', label: 'Users' },
  { key: 'invites', label: 'Invites' },
  { key: 'config', label: 'System Config' },
]

export default function Admin() {
  const [activeTab, setActiveTab] = useState<Tab>('overview')

  return (
    <div>
      <h1
        className="mb-4 text-lg font-bold"
        style={{ color: 'var(--color-text-primary)', fontFamily: "'Outfit', system-ui, sans-serif" }}
      >
        Admin Panel
      </h1>

      {/* Tab bar */}
      <div className="glass mb-5 flex gap-0.5 rounded-lg p-1">
        {TABS.map((tab) => {
          const active = activeTab === tab.key
          return (
            <button
              key={tab.key}
              onClick={() => setActiveTab(tab.key)}
              className={`flex-1 rounded-md px-3 py-2 text-sm font-medium transition-all duration-200 ${
                active
                  ? 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)] shadow-sm'
                  : 'text-[var(--color-text-muted)] hover:text-[var(--color-text-secondary)]'
              }`}
            >
              {tab.label}
            </button>
          )
        })}
      </div>

      {activeTab === 'overview' && <AdminStats />}
      {activeTab === 'users' && <AdminUsers />}
      {activeTab === 'invites' && <AdminInvites />}
      {activeTab === 'config' && <SystemConfig />}
    </div>
  )
}
