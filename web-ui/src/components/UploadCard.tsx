import { Check, Loader2, X, AlertTriangle } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useFormat } from '../hooks/useFormat'
import type { UploadTask } from '../hooks/useUploadQueue'

interface UploadCardProps {
  task: UploadTask
}

const STATUS_ICONS: Record<UploadTask['status'], { icon: typeof Check; cls: string }> = {
  pending: { icon: Loader2, cls: 'text-[var(--color-text-muted)]' },
  processing: { icon: Loader2, cls: 'animate-spin text-[var(--color-accent)]' },
  uploading: { icon: Loader2, cls: 'animate-spin text-[var(--color-accent)]' },
  done: { icon: Check, cls: 'text-[var(--color-success)]' },
  error: { icon: X, cls: 'text-[var(--color-danger)]' },
}

const STATUS_KEYS: Record<
  UploadTask['status'],
  'upload.queued' | 'upload.processing' | 'upload.uploading' | 'upload.uploaded' | 'upload.failed'
> = {
  pending: 'upload.queued',
  processing: 'upload.processing',
  uploading: 'upload.uploading',
  done: 'upload.uploaded',
  error: 'upload.failed',
}

export default function UploadCard({ task }: UploadCardProps) {
  const { t } = useTranslation()
  const { formatBytes } = useFormat()
  const { icon: Icon, cls: iconCls } = STATUS_ICONS[task.status]

  return (
    <div className="glass group flex items-center gap-3 rounded-lg p-3">
      {/* Status icon */}
      <Icon className={`h-5 w-5 shrink-0 ${iconCls}`} />

      {/* File info */}
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm" style={{ color: 'var(--color-text-primary)' }}>
          {task.file.name}
        </p>
        {task.status === 'done' && task.result?.storage_config && (
          <p className="truncate text-xs" style={{ color: 'var(--color-text-secondary)' }}>
            → {task.result.storage_config.name}
          </p>
        )}
        <div className="mt-1.5 flex items-center gap-2">
          {/* Progress bar */}
          {(task.status === 'pending' || task.status === 'uploading') && (
            <div className="h-1 flex-1 overflow-hidden rounded-full" style={{ backgroundColor: 'var(--color-border)' }}>
              <div
                className="h-full rounded-full transition-all duration-700"
                style={{
                  width: task.status === 'uploading'
                    ? `${20 + Math.min(task.progress, 100) * 0.8}%`
                    : '0%',
                  backgroundColor: 'var(--color-accent)',
                  ...(task.status === 'uploading' && task.progress === 0
                    ? { animation: 'uploadPulse 1.5s ease-in-out infinite' }
                    : {}),
                }}
              />
            </div>
          )}
          <span className="text-xs" style={{ color: 'var(--color-text-muted)' }}>
            {t(STATUS_KEYS[task.status])}
          </span>
          {task.status === 'done' && task.result && (
            <span className="text-xs" style={{ color: 'var(--color-text-muted)' }}>
              {formatBytes(task.result.file_size)}
            </span>
          )}
          {task.status === 'error' && task.error && (
            <span className="flex items-center gap-1 text-xs" style={{ color: 'var(--color-danger)' }}>
              <AlertTriangle className="h-3 w-3" />
              {task.error}
            </span>
          )}
        </div>
        {task.status === 'done' && task.result && (
          <div className="mt-1.5 flex flex-wrap gap-2">
            <a
              href={task.result.url}
              target="_blank"
              rel="noopener noreferrer"
              className="text-xs font-medium underline underline-offset-2 hover:opacity-80"
              style={{ color: 'var(--color-accent)' }}
            >
              {t('upload.open')}
            </a>
            <button
              onClick={() => navigator.clipboard.writeText(task.result!.url)}
              className="text-xs font-medium underline underline-offset-2 hover:opacity-80"
              style={{ color: 'var(--color-accent)' }}
            >
              {t('upload.copyUrl')}
            </button>
            <button
              onClick={() => navigator.clipboard.writeText(task.result!.markdown)}
              className="text-xs underline underline-offset-2 hover:opacity-80"
              style={{ color: 'var(--color-text-muted)' }}
            >
              {t('upload.copyMd')}
            </button>
          </div>
        )}
      </div>

      {/* Thumbnail */}
      {task.status === 'done' && task.result && (
        <img
          src={task.result.url}
          alt={task.file.name}
          className="h-10 w-10 shrink-0 rounded-md object-cover ring-1 ring-white/5"
        />
      )}
    </div>
  )
}
