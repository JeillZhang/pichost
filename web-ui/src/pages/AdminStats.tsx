import { useQuery } from '@tanstack/react-query'
import { Users, Image as ImageIcon, HardDrive, Activity } from 'lucide-react'
import api from '../api/client'

interface BackendStats {
  total_images: number
  total_size: number
}

interface AdminStatsResponse {
  total_users: number
  total_images: number
  total_size: number
  active_users_24h: number
  storage_backends: Record<string, BackendStats>
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return `${(bytes / Math.pow(k, i)).toFixed(1)} ${sizes[i]}`
}

type StatKey = 'total_users' | 'total_images' | 'total_size' | 'active_users_24h'

interface StatCard {
  key: StatKey
  label: string
  icon: typeof Users
  color: string
  format?: (v: number) => string
}

const statCards: StatCard[] = [
  { key: 'total_users', label: 'Total Users', icon: Users, color: '#3b82f6' },
  { key: 'total_images', label: 'Total Images', icon: ImageIcon, color: '#8b5cf6' },
  { key: 'total_size', label: 'Total Storage', icon: HardDrive, color: '#22c55e', format: (v: number) => formatBytes(v) },
  { key: 'active_users_24h', label: 'Active (24h)', icon: Activity, color: '#f59e0b' },
]

export default function AdminStats() {
  const { data, isLoading } = useQuery({
    queryKey: ['admin', 'stats'],
    queryFn: () => api.get('admin/stats').json<AdminStatsResponse>(),
    refetchInterval: 30_000,
  })

  if (isLoading || !data) {
    return (
      <div className="flex items-center justify-center py-20" style={{ color: 'var(--color-text-muted)' }}>
        Loading stats…
      </div>
    )
  }

  return (
    <div>
      <div className="grid grid-cols-2 gap-4 lg:grid-cols-4">
        {statCards.map(({ key, label, icon: Icon, color, format }) => {
          const value = data[key]
          return (
            <div
              key={key}
              className="rounded-xl p-4"
              style={{
                backgroundColor: 'var(--glass-bg)',
                border: '1px solid var(--glass-border)',
                backdropFilter: 'blur(var(--glass-blur))',
              }}
            >
              <div className="flex items-center justify-between">
                <span className="text-xs font-medium uppercase tracking-wide" style={{ color: 'var(--color-text-muted)' }}>
                  {label}
                </span>
                <Icon className="h-4 w-4" style={{ color }} />
              </div>
              <p className="mt-2 text-2xl font-bold" style={{ color: 'var(--color-text-primary)' }}>
                {format ? format(value) : value.toLocaleString()}
              </p>
            </div>
          )
        })}
      </div>

      {/* Backend breakdown */}
      <div
        className="mt-6 rounded-xl p-4"
        style={{
          backgroundColor: 'var(--glass-bg)',
          border: '1px solid var(--glass-border)',
          backdropFilter: 'blur(var(--glass-blur))',
        }}
      >
        <h3 className="mb-3 text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>
          Storage Backend Breakdown
        </h3>
        <div className="space-y-3">
          {Object.entries(data.storage_backends).map(([name, stats]) => (
            <div key={name}>
              <div className="mb-1 flex justify-between text-sm">
                <span style={{ color: 'var(--color-text-primary)' }}>{name}</span>
                <span style={{ color: 'var(--color-text-muted)' }}>
                  {stats.total_images.toLocaleString()} images / {formatBytes(stats.total_size)}
                </span>
              </div>
              <div
                className="h-2 overflow-hidden rounded-full"
                style={{ backgroundColor: 'var(--color-surface)' }}
              >
                <div
                  className="h-full rounded-full transition-all"
                  style={{
                    width: `${data.total_images > 0 ? (stats.total_images / data.total_images) * 100 : 0}%`,
                    backgroundColor: name === 'local' ? '#3b82f6' : '#8b5cf6',
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
