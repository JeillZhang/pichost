import { useState } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import { useQuery, useQueryClient } from '@tanstack/react-query'
import { ArrowLeft, Trash2 } from 'lucide-react'
import { toast } from 'sonner'
import NavBar from '../components/NavBar'
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
        return 'bg-gray-800 text-gray-400 border-gray-600'
    }
  }

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center text-gray-500">
        Loading…
      </div>
    )
  }

  if (!img) {
    return (
      <div className="flex min-h-screen items-center justify-center text-gray-600">
        Image not found.
      </div>
    )
  }

  return (
    <>
      <NavBar />
      <div className="mx-auto max-w-2xl p-4">
      {/* Back button */}
      <button
        onClick={() => navigate(-1)}
        className="mb-4 flex items-center gap-1.5 text-sm text-gray-400 hover:text-gray-200"
      >
        <ArrowLeft className="h-4 w-4" />
        Back
      </button>

      {/* Image preview */}
      <div className="overflow-hidden rounded-xl border border-gray-800 bg-gray-900/50">
        <img
          src={img.url}
          alt={img.original_name}
          className="max-h-[60vh] w-full object-contain"
        />
      </div>

      {/* Info */}
      <div className="mt-4 space-y-1 text-sm text-gray-400">
        <p>
          Name:{' '}
          <span className="text-gray-200">{img.original_name}</span>
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
            <span className="text-gray-200">
              {img.width} × {img.height}px
            </span>
          </p>
        )}
        <p>
          Type:{' '}
          <span className="text-gray-200">{img.mime_type}</span>
        </p>
        <p>
          Size:{' '}
          <span className="text-gray-200">
            {(img.file_size / 1024).toFixed(1)} KB
          </span>
        </p>
        <p>
          Uploaded:{' '}
          <span className="text-gray-200">
            {new Date(img.created_at).toLocaleString()}
          </span>
        </p>
      </div>

      {/* Additional links */}
      {(img.thumbnail_url || img.webp_url) && (
        <div className="mt-4 space-y-2">
          <p className="text-xs font-medium uppercase tracking-wide text-gray-500">
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
      <div className="mt-6 border-t border-gray-800 pt-4">
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
              className="rounded-lg px-4 py-2 text-sm text-gray-400 hover:bg-gray-800 hover:text-gray-200"
            >
              Cancel
            </button>
          </div>
        ) : (
          <button
            onClick={handleDelete}
            className="flex items-center gap-1.5 rounded-lg border border-gray-700 px-4 py-2 text-sm text-red-400 hover:bg-red-900/30 hover:text-red-300"
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
