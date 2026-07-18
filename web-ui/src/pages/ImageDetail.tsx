import { useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import { getImage, deleteImage } from '../api/client'
import LinkCard from '../components/LinkCard'

export default function ImageDetail() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const queryClient = useQueryClient()
  const [deleting, setDeleting] = useState(false)
  const [confirmDelete, setConfirmDelete] = useState(false)

  const { data: img, isLoading } = useQuery({
    queryKey: ['image', id],
    queryFn: () => getImage(id!),
    enabled: !!id,
  })

  async function handleDelete() {
    if (!confirmDelete) {
      setConfirmDelete(true)
      return
    }
    if (!id) return
    setDeleting(true)
    try {
      await deleteImage(id)
      toast.success('Image deleted')
      queryClient.invalidateQueries({ queryKey: ['images'] })
      navigate('/dashboard', { replace: true })
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Delete failed'
      toast.error(msg)
      setDeleting(false)
      setConfirmDelete(false)
    }
  }

  function statusColor(status: string): string {
    switch (status) {
      case 'active':
      case 'ready':
        return 'bg-green-900/50 text-green-400 border-green-700'
      case 'processing':
        return 'bg-yellow-900/50 text-yellow-400 border-yellow-700'
      case 'pending':
        return 'bg-blue-900/50 text-blue-400 border-blue-700'
      case 'failed':
        return 'bg-red-900/50 text-red-400 border-red-700'
      default:
        return 'bg-[var(--color-surface)] text-[var(--color-text-secondary)] border-[var(--color-border)]'
    }
  }

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center text-[var(--color-text-muted)]">
        Loading…
      </div>
    )
  }

  if (!img) {
    return (
      <div className="flex min-h-screen items-center justify-center text-[var(--color-text-muted)]">
        Image not found.
      </div>
    )
  }

  return (
    <>
      <div className="mx-auto max-w-2xl p-4">
      {/* Back button */}
      <button
        onClick={() => navigate(-1)}
        className="mb-4 flex items-center gap-1.5 text-sm text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]"
      >
        <ArrowLeft className="h-4 w-4" />
        Back
      </button>

      {/* Image preview */}
      <div className="overflow-hidden rounded-xl border border-[var(--glass-border)] bg-[var(--glass-bg)]" style={{ backdropFilter: 'blur(var(--glass-blur))', boxShadow: 'var(--glass-shadow)' }}>
        <img
          src={img.url}
          alt={img.original_name}
          className="max-h-[60vh] w-full object-contain"
        />
      </div>

      {/* Info */}
      <div className="mt-4 rounded-xl border border-[var(--glass-border)] bg-[var(--glass-bg)] p-4 space-y-1 text-sm text-[var(--color-text-secondary)]" style={{ backdropFilter: 'blur(var(--glass-blur))', boxShadow: 'var(--glass-shadow)' }}>
        <p>
          Name:{' '}
          <span className="text-[var(--color-text-primary)]">{img.original_name}</span>
        </p>
        <p className="flex items-center gap-2">
          Status:{' '}
          <span
            className={`rounded border px-2 py-0.5 text-xs font-medium ${statusColor(img.status)}`}
          >
            {img.status}
          </span>
        </p>
        {img.width && img.height && (
          <p>
            Dimensions:{' '}
            <span className="text-[var(--color-text-primary)]">
              {img.width} × {img.height}px
            </span>
          </p>
        )}
        <p>
          Type:{' '}
          <span className="text-[var(--color-text-primary)]">{img.mime_type}</span>
        </p>
        <p>
          Size:{' '}
          <span className="text-[var(--color-text-primary)]">
            {(img.file_size / 1024).toFixed(1)} KB
          </span>
        </p>
        <p>
          Uploaded:{' '}
          <span className="text-[var(--color-text-primary)]">
            {new Date(img.created_at).toLocaleString()}
          </span>
        </p>
      </div>

      {/* Additional links */}
      {(img.thumbnail_url || img.webp_url) && (
        <div className="mt-4 space-y-2">
          <p className="text-xs font-medium uppercase tracking-wide text-[var(--color-text-muted)]">
            Generated Assets
          </p>
          {img.thumbnail_url && (
            <LinkCard label="Thumbnail URL" value={img.thumbnail_url} />
          )}
          {img.webp_url && (
            <LinkCard label="WebP URL" value={img.webp_url} />
          )}
        </div>
      )}

      {/* Links */}
      <div className="mt-4 space-y-2">
        <LinkCard label="URL" value={img.url} />
        <LinkCard label="Markdown" value={img.markdown} />
        <LinkCard label="HTML" value={img.html} />
        <LinkCard label="BBCode" value={img.bbcode} />
      </div>

      {/* Delete */}
      <div className="mt-6 border-t border-[var(--color-border)] pt-4">
        {confirmDelete ? (
          <div className="flex items-center gap-3">
            <button
              onClick={handleDelete}
              disabled={deleting}
              className="flex items-center gap-1.5 rounded-lg bg-red-600 px-4 py-2 text-sm font-medium text-white hover:bg-red-700 disabled:opacity-50"
            >
              <Trash2 className="h-4 w-4" />
              {deleting ? 'Deleting…' : 'Confirm Delete'}
            </button>
            <button
              onClick={() => setConfirmDelete(false)}
              className="rounded-lg px-4 py-2 text-sm text-[var(--color-text-secondary)] hover:bg-[var(--color-surface)] hover:text-[var(--color-text-primary)]"
            >
              Cancel
            </button>
          </div>
        ) : (
          <button
            onClick={handleDelete}
            className="flex items-center gap-1.5 rounded-lg border border-[var(--color-border)] px-4 py-2 text-sm text-[var(--color-danger)] hover:bg-[var(--color-danger-subtle)] hover:text-[var(--color-danger-hover)]"
          >
            <Trash2 className="h-4 w-4" />
            Delete Image
          </button>
        )}
      </div>
      </div>
    </>
  )
}
