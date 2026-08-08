import { useQuery } from '@tanstack/react-query'
import { Users, Image as ImageIcon, HardDrive, Activity } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useFormat } from '../../hooks/useFormat'
import api from '../../api/client'

interface BackendStats {
  total_images: number
  total_size: number
}

interface AdminStatsResponse {
  total_users: number
  total_images: number
  total_size: number
  active_users_24h: number
  total_quota: number | null
  storage_backends: Record<string, BackendStats>
}

type StatKey = 'total_users' | 'total_images' | 'total_size' | 'active_users_24h'
type StatsLabelKey =
  | 'adminStats.totalUsers'
  | 'adminStats.totalImages'
  | 'adminStats.totalStorage'
  | 'adminStats.active24h'

interface StatCard {
  key: StatKey
  labelKey: StatsLabelKey
  icon: typeof Users
  color: string
}

const statCards: StatCard[] = [
  { key: 'total_users', labelKey: 'adminStats.totalUsers', icon: Users, color: 'var(--color-accent)' },
  { key: 'total_images', labelKey: 'adminStats.totalImages', icon: ImageIcon, color: 'var(--color-accent)' },
  { key: 'total_size', labelKey: 'adminStats.totalStorage', icon: HardDrive, color: 'var(--color-success)' },
  { key: 'active_users_24h', labelKey: 'adminStats.active24h', icon: Activity, color: 'var(--color-warning)' },
]

export default function AdminStats() {
  const { t } = useTranslation()
  const { formatBytes, formatNumber } = useFormat()
  const { data, isLoading } = useQuery({
    queryKey: ['admin', 'stats'],
    queryFn: () => api.get('admin/stats').json<AdminStatsResponse>(),
    refetchInterval: 30_000,
  })

  if (isLoading || !data) {
    return (
      <div className="flex items-center justify-center py-20" style={{ color: 'var(--color-text-muted)' }}>
        {t('adminStats.loading')}
      </div>
    )
  }

  return (
    <div>
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        {statCards.map(({ key, labelKey, icon: Icon, color }) => {
          const value = data[key]
          return (
            <div key={key} className="glass rounded-xl p-4">
              <div className="flex items-center justify-between">
                <span className="text-xs font-medium uppercase tracking-wide" style={{ color: 'var(--color-text-muted)' }}>
                  {t(labelKey)}
                </span>
                <Icon className="h-4 w-4" style={{ color }} />
              </div>
              <p className="mt-2 text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
                {key === 'total_size' ? formatBytes(value) : formatNumber(value)}
              </p>
            </div>
          )
        })}
      </div>

      {/* Backend breakdown */}
      <div className="glass mt-6 rounded-xl p-4">
        <h3 className="mb-3 text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>
          {t('adminStats.storageBreakdown')}
        </h3>
        <div className="space-y-3">
          {Object.entries(data.storage_backends).map(([name, stats]) => (
            <div key={name}>
              <div className="mb-1 flex justify-between text-sm">
                <span style={{ color: 'var(--color-text-primary)' }}>{name}</span>
                <span style={{ color: 'var(--color-text-muted)' }}>
                  {formatNumber(stats.total_images)} {t('adminStats.backendImages')} / {formatBytes(stats.total_size)}
                  {data.total_quota ? ` / ${formatBytes(data.total_quota)}` : ''}
                </span>
              </div>
              <div
                className="h-2 overflow-hidden rounded-full"
                style={{ backgroundColor: 'var(--color-surface)' }}
              >
                <div
                  className="h-full rounded-full transition-all"
                  style={{
                    width: `${
                      data.total_quota && data.total_quota > 0
                        ? Math.min(100, (stats.total_size / data.total_quota) * 100)
                        : 0
                    }%`,
                    backgroundColor: 'var(--color-accent)',
                  }}
                />
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  )
}
