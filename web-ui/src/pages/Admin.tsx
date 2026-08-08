import { useState } from 'react'
import { useTranslation } from 'react-i18next'
import AdminStats from './admin/AdminStats'
import AdminUsers from './admin/AdminUsers'
import AdminInvites from './admin/AdminInvites'
import SystemConfig from '../components/SystemConfig'

type Tab = 'overview' | 'users' | 'invites' | 'config'
type TabLabelKey = 'adminTabs.overview' | 'adminTabs.users' | 'adminTabs.invites' | 'adminTabs.systemConfig'

const TABS: { key: Tab; labelKey: TabLabelKey }[] = [
  { key: 'overview', labelKey: 'adminTabs.overview' },
  { key: 'users', labelKey: 'adminTabs.users' },
  { key: 'invites', labelKey: 'adminTabs.invites' },
  { key: 'config', labelKey: 'adminTabs.systemConfig' },
]

export default function Admin() {
  const { t } = useTranslation()
  const [activeTab, setActiveTab] = useState<Tab>('overview')

  return (
    <div>
      <h1
        className="mb-4 text-lg font-bold"
        style={{ color: 'var(--color-text-primary)', fontFamily: "'Outfit', system-ui, sans-serif" }}
      >
        {t('adminTabs.title')}
      </h1>

      {/* Tab bar */}
      <div className="glass mb-5 flex gap-0.5 overflow-x-auto rounded-lg p-1">
        {TABS.map((tab) => {
          const active = activeTab === tab.key
          return (
            <button
              key={tab.key}
              onClick={() => setActiveTab(tab.key)}
              className={`flex-1 whitespace-nowrap rounded-md px-3 py-2 text-sm font-medium transition-all duration-200 ${
                active
                  ? 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)] shadow-sm'
                  : 'text-[var(--color-text-muted)] hover:text-[var(--color-text-secondary)]'
              }`}
            >
              {t(tab.labelKey)}
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
