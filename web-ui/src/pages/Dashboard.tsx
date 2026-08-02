import { useRef, useEffect, useState, useCallback } from 'react'
import { useNavigate } from 'react-router-dom'
import { Shield, Trash2, Plus, X } from 'lucide-react'
import { useAuthStore } from '../stores/auth'
import DropZone from '../components/DropZone'
import UploadCard from '../components/UploadCard'
import { listImages, getUserStats, listStorageConfigs, uploadFromUrl } from '../api/client'
import type { UserStorageConfig } from '../api/client'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useUploadQueue } from '../hooks/useUploadQueue'
import { useClipboardPaste } from '../hooks/useClipboardPaste'
import UrlUploadInput from '../components/UrlUploadInput'
import { PreprocessingStatus } from '../components/PreprocessingStatus'
import GlassSelect from '../components/ui/GlassSelect'

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), units.length - 1)
  return `${(bytes / Math.pow(1024, i)).toFixed(i === 0 ? 0 : 1)} ${units[i]}`
}

export default function Dashboard() {
  const user = useAuthStore((s) => s.user)
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const { queue, addFiles, clearQueue } = useUploadQueue()

  const { data } = useQuery({
    queryKey: ['images'],
    queryFn: () => listImages({ per_page: 50 }),
  })
  const images = data?.items

  const { data: stats } = useQuery({
    queryKey: ['user-stats'],
    queryFn: () => getUserStats(),
  })

  const { data: storageConfigs } = useQuery({
    queryKey: ['storage-configs'],
    queryFn: () => listStorageConfigs(),
  })
  const [selectedConfigIds, setSelectedConfigIds] = useState<string[]>([])

  useEffect(() => {
    if (storageConfigs && storageConfigs.length > 0 && selectedConfigIds.length === 0) {
      const defaultCfg = storageConfigs.find((c) => c.is_default) || storageConfigs[0]
      setSelectedConfigIds([defaultCfg.id])
    }
  }, [storageConfigs, selectedConfigIds.length])

  const handleUpload = (files: File[]) => {
    addFiles(files, selectedConfigIds.length > 0 ? selectedConfigIds : undefined)
  }

  const handlePaste = useCallback(
    (files: File[]) => {
      addFiles(files, selectedConfigIds.length > 0 ? selectedConfigIds : undefined)
    },
    [addFiles, selectedConfigIds],
  )
  useClipboardPaste(handlePaste)

  const handleUrlUpload = useCallback(
    async (url: string) => {
      await uploadFromUrl(url, selectedConfigIds.length > 0 ? selectedConfigIds : undefined)
      queryClient.invalidateQueries({ queryKey: ['images'] })
    },
    [selectedConfigIds, queryClient],
  )

  const availableForSlot = (slotIndex: number): UserStorageConfig[] =>
    (storageConfigs || []).filter(
      (c) => !selectedConfigIds.includes(c.id) || c.id === selectedConfigIds[slotIndex],
    )

  const handleSlotChange = (slotIndex: number, newId: string) => {
    setSelectedConfigIds((prev) => {
      const next = [...prev]
      next[slotIndex] = newId
      return next
    })
  }

  const handleRemoveSlot = (slotIndex: number) => {
    setSelectedConfigIds((prev) => {
      if (prev.length === 2 && slotIndex === 0) return [prev[1]]
      if (prev.length === 2 && slotIndex === 1) return [prev[0]]
      return []
    })
  }

  const handleAddSlot = () => {
    if (!storageConfigs || selectedConfigIds.length >= 2) return
    const next = storageConfigs.find((c) => !selectedConfigIds.includes(c.id))
    if (next) setSelectedConfigIds((prev) => [...prev, next.id])
  }

  const canAddSlot =
    storageConfigs &&
    selectedConfigIds.length < 2 &&
    selectedConfigIds.length < storageConfigs.length

  // Invalidate when any upload completes
  const prevDoneCount = useRef(0)
  const doneCount = queue.filter((t) => t.status === 'done').length
  useEffect(() => {
    if (doneCount > prevDoneCount.current) {
      queryClient.invalidateQueries({ queryKey: ['images'] })
    }
    prevDoneCount.current = doneCount
  }, [doneCount, queryClient])

  const hasActiveUploads = queue.some(
    (t) => t.status === 'pending' || t.status === 'uploading',
  )

  return (
    <div className="mx-auto max-w-2xl p-4">
      {/* Admin banner */}
      {user?.is_admin && (
        <div
          className="glass mb-4 flex items-center gap-2 rounded-lg px-4 py-3 text-sm"
          style={{
            borderColor: 'var(--color-accent-strong)',
            backgroundColor: 'var(--color-accent-subtle)',
            color: 'var(--color-accent)',
          }}
        >
          <Shield className="h-4 w-4 shrink-0" />
          <span>
            You are an administrator.{' '}
            <button
              onClick={() => navigate('/admin')}
              className="font-semibold underline underline-offset-2 hover:opacity-80"
            >
              Go to Admin Panel
            </button>
          </span>
        </div>
      )}

      {/* Backend selector */}
      {storageConfigs && storageConfigs.length > 0 && (
        <div className="mb-4 space-y-2">
          <div className="flex items-center justify-between">
            <span
              className="text-xs font-semibold uppercase tracking-wider"
              style={{ color: 'var(--color-text-muted)' }}
            >
              Storage Backend
            </span>
            {canAddSlot && (
              <button
                onClick={handleAddSlot}
                className="flex items-center gap-1 rounded-lg px-2 py-1 text-xs font-medium transition-colors duration-150 hover:bg-[var(--color-accent-subtle)]"
                style={{ color: 'var(--color-accent)' }}
              >
                <Plus className="h-3 w-3" />
                Add 2nd backend
              </button>
            )}
          </div>
          {selectedConfigIds.map((selectedId, idx) => (
            <div key={idx} className="relative flex items-center gap-2">
              <div className="flex-1">
                <GlassSelect
                  value={selectedId}
                  onChange={(v) => handleSlotChange(idx, v)}
                  options={availableForSlot(idx).map((cfg) => ({
                    value: cfg.id,
                    label: `${cfg.name} (${cfg.provider})${cfg.is_default ? ' · default' : ''}`,
                  }))}
                />
              </div>
              {selectedConfigIds.length > 1 && (
                <button
                  onClick={() => handleRemoveSlot(idx)}
                  className="shrink-0 rounded-lg p-1.5 transition-colors duration-150 hover:bg-[var(--color-surface-hover)]"
                  style={{ color: 'var(--color-text-muted)' }}
                >
                  <X className="h-4 w-4" />
                </button>
              )}
            </div>
          ))}
        </div>
      )}

      {/* DropZone */}
      <DropZone onUpload={handleUpload} />

      <div className="mt-3">
        <UrlUploadInput onUpload={handleUrlUpload} />
      </div>

      {/* Preprocessing status */}
      <div className="mt-3 flex justify-end">
        <PreprocessingStatus />
      </div>

      {/* Upload queue */}
      {queue.length > 0 && (
        <div className="mt-5 space-y-2">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>
              Uploads
              {hasActiveUploads && (
                <span className="ml-2 text-xs" style={{ color: 'var(--color-text-muted)' }}>
                  {queue.filter((t) => t.status === 'pending' || t.status === 'uploading').length}{' '}
                  active
                </span>
              )}
            </h2>
            {queue.some((t) => t.status === 'done' || t.status === 'error') && (
              <button
                onClick={clearQueue}
                className="flex items-center gap-1 rounded-lg px-2 py-1 text-xs transition-colors duration-150 hover:bg-[var(--color-surface-hover)]"
                style={{ color: 'var(--color-text-muted)' }}
              >
                <Trash2 className="h-3 w-3" />
                Clear done
              </button>
            )}
          </div>
          {queue.map((task) => (
            <UploadCard key={task.id} task={task} />
          ))}
        </div>
      )}

      {/* Storage usage bar */}
      {stats && stats.storage_quota != null && (
        <div className="glass mt-5 rounded-lg p-3">
          <div className="mb-1.5 flex items-center justify-between text-xs">
            <span style={{ color: 'var(--color-text-muted)' }}>Storage</span>
            <span style={{ color: 'var(--color-text-secondary)' }}>
              {formatBytes(stats.total_size)} / {formatBytes(stats.storage_quota)}
            </span>
          </div>
          <div
            className="h-2 overflow-hidden rounded-full"
            style={{ backgroundColor: 'var(--color-surface)' }}
          >
            <div
              className="h-full rounded-full transition-all duration-500"
              style={{
                width: `${Math.min(100, (stats.total_size / stats.storage_quota) * 100)}%`,
                backgroundColor:
                  stats.total_size / stats.storage_quota > 0.9
                    ? 'var(--color-danger)'
                    : stats.total_size / stats.storage_quota > 0.7
                      ? 'var(--color-warning)'
                      : 'var(--color-accent)',
              }}
            />
          </div>
        </div>
      )}

      {/* Recent images */}
      {images && images.length > 0 && (
        <div className="mt-8">
          <h2
            className="mb-3 text-sm font-semibold uppercase tracking-wider"
            style={{ color: 'var(--color-text-muted)' }}
          >
            Recent
          </h2>
          <div className="space-y-2">
            {images.map((img) => (
              <div
                key={img.id}
                className="glass group flex items-center gap-3 rounded-lg p-3"
              >
                <img
                  src={img.url}
                  alt={img.original_name}
                  className="h-12 w-12 shrink-0 rounded-lg object-cover ring-1 ring-white/5"
                />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm" style={{ color: 'var(--color-text-primary)' }}>
                    {img.original_name}
                  </p>
                  <p className="text-xs" style={{ color: 'var(--color-text-muted)' }}>
                    {(img.file_size / 1024).toFixed(1)} KB
                  </p>
                </div>
                <button
                  onClick={() => navigate(`/images/${img.id}`)}
                  className="btn-ghost shrink-0 px-3 py-1.5 text-xs"
                >
                  Detail
                </button>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* Empty state */}
      {images && images.length === 0 && queue.length === 0 && (
        <div className="mt-8 text-center text-sm" style={{ color: 'var(--color-text-muted)' }}>
          No images yet. Upload one above!
        </div>
      )}
    </div>
  )
}
