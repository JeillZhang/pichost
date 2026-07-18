import { useRef, useEffect } from 'react'
import { useNavigate } from 'react-router-dom'
import { Shield, Trash2 } from 'lucide-react'
import { useAuthStore } from '../stores/auth'
import DropZone from '../components/DropZone'
import UploadCard from '../components/UploadCard'
import { listImages } from '../api/client'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { useUploadQueue } from '../hooks/useUploadQueue'

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
          className="mb-4 flex items-center gap-2 rounded-lg px-4 py-3 text-sm"
          style={{
            backgroundColor: 'var(--color-accent-subtle)',
            border: '1px solid var(--color-accent)',
            color: 'var(--color-accent)',
          }}
        >
          <Shield className="h-4 w-4 shrink-0" />
          <span>
            You are an administrator.{' '}
            <button
              onClick={() => navigate('/admin')}
              className="font-medium underline underline-offset-2 hover:opacity-80"
            >
              Go to Admin Panel
            </button>
          </span>
        </div>
      )}

      {/* DropZone — always active, accepts multiple files */}
      <DropZone onUpload={addFiles} />

      {/* Upload queue */}
      {queue.length > 0 && (
        <div className="mt-4 space-y-2">
          <div className="flex items-center justify-between">
            <h2 className="text-sm font-medium text-[var(--color-text-secondary)]">
              Uploads
              {hasActiveUploads && (
                <span className="ml-2 text-xs text-[var(--color-text-muted)]">
                  {queue.filter((t) => t.status === 'pending' || t.status === 'uploading').length} active
                </span>
              )}
            </h2>
            {queue.some((t) => t.status === 'done' || t.status === 'error') && (
              <button
                onClick={clearQueue}
                className="flex items-center gap-1 rounded px-2 py-1 text-xs text-[var(--color-text-muted)] hover:bg-[var(--color-surface)] hover:text-[var(--color-text-primary)]"
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

      {/* Recent images */}
      {images && images.length > 0 && (
        <div className="mt-8">
          <h2 className="mb-3 text-sm font-medium text-[var(--color-text-secondary)]">Recent</h2>
          <div className="space-y-2">
            {images.map((img) => (
              <div
                key={img.id}
                className="flex items-center gap-3 rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-3 backdrop-blur-sm"
              >
                <img
                  src={img.url}
                  alt={img.original_name}
                  className="h-12 w-12 shrink-0 rounded object-cover"
                />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm text-[var(--color-text-primary)]">
                    {img.original_name}
                  </p>
                  <p className="text-xs text-[var(--color-text-muted)]">
                    {(img.file_size / 1024).toFixed(1)} KB
                  </p>
                </div>
                <button
                  onClick={() => navigate(`/images/${img.id}`)}
                  className="shrink-0 rounded px-3 py-1.5 text-xs text-[var(--color-text-secondary)] hover:bg-[var(--color-surface)] hover:text-[var(--color-text-primary)]"
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
        <div className="mt-8 text-center text-sm text-[var(--color-text-muted)]">
          No images yet. Upload one above!
        </div>
      )}
    </div>
  )
}
