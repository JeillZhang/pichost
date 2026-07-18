import { Check, Loader2, X, AlertTriangle } from 'lucide-react'
import type { UploadTask } from '../hooks/useUploadQueue'

interface UploadCardProps {
  task: UploadTask
}

const STATUS_ICONS: Record<UploadTask['status'], { icon: typeof Check; cls: string }> = {
  pending: { icon: Loader2, cls: 'text-[var(--color-text-muted)]' },
  uploading: { icon: Loader2, cls: 'animate-spin text-blue-400' },
  done: { icon: Check, cls: 'text-green-400' },
  error: { icon: X, cls: 'text-red-400' },
}

const STATUS_LABELS: Record<UploadTask['status'], string> = {
  pending: 'Queued',
  uploading: 'Uploading…',
  done: 'Uploaded',
  error: 'Failed',
}

export default function UploadCard({ task }: UploadCardProps) {
  const { icon: Icon, cls: iconCls } = STATUS_ICONS[task.status]

  return (
    <div className="flex items-center gap-3 rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-3 backdrop-blur-sm">
      {/* Status icon */}
      <Icon className={`h-5 w-5 shrink-0 ${iconCls}`} />

      {/* File info */}
      <div className="min-w-0 flex-1">
        <p className="truncate text-sm text-[var(--color-text-primary)]">
          {task.file.name}
        </p>
        <div className="mt-1 flex items-center gap-2">
          {/* Progress bar (pending/uploading) */}
          {(task.status === 'pending' || task.status === 'uploading') && (
            <div className="h-1 flex-1 overflow-hidden rounded-full bg-[var(--color-border)]">
              <div
                className="h-full rounded-full bg-[var(--color-accent)] transition-all duration-300"
                style={{ width: `${task.status === 'uploading' ? 60 : 0}%` }}
              />
            </div>
          )}
          {/* Status label */}
          <span className="text-xs text-[var(--color-text-muted)]">
            {STATUS_LABELS[task.status]}
          </span>
          {/* Done — show file size */}
          {task.status === 'done' && task.result && (
            <span className="text-xs text-[var(--color-text-muted)]">
              {(task.result.file_size / 1024).toFixed(1)} KB
            </span>
          )}
          {/* Error — show message */}
          {task.status === 'error' && task.error && (
            <span className="flex items-center gap-1 text-xs text-red-400">
              <AlertTriangle className="h-3 w-3" />
              {task.error}
            </span>
          )}
        </div>
        {/* Done — show result links */}
        {task.status === 'done' && task.result && (
          <div className="mt-1 flex flex-wrap gap-2">
            <a
              href={task.result.url}
              target="_blank"
              rel="noopener noreferrer"
              className="text-xs text-[var(--color-accent)] underline underline-offset-2 hover:opacity-80"
            >
              Open
            </a>
            <button
              onClick={() => navigator.clipboard.writeText(task.result!.url)}
              className="text-xs text-[var(--color-accent)] underline underline-offset-2 hover:opacity-80"
            >
              Copy URL
            </button>
            <button
              onClick={() => navigator.clipboard.writeText(task.result!.markdown)}
              className="text-xs text-[var(--color-text-muted)] underline underline-offset-2 hover:opacity-80"
            >
              Copy MD
            </button>
          </div>
        )}
      </div>

      {/* Thumbnail preview (done only) */}
      {task.status === 'done' && task.result && (
        <img
          src={task.result.url}
          alt={task.file.name}
          className="h-10 w-10 shrink-0 rounded object-cover"
        />
      )}
    </div>
  )
}
