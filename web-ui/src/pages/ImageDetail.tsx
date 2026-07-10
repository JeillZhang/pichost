import { useParams, useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { ArrowLeft } from 'lucide-react'
import { getImage } from '../api/client'
import LinkCard from '../components/LinkCard'

export default function ImageDetail() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()

  const { data: img, isLoading } = useQuery({
    queryKey: ['image', id],
    queryFn: () => getImage(id!),
    enabled: !!id,
  })

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
      </div>

      {/* Links */}
      <div className="mt-4 space-y-2">
        <LinkCard label="URL" value={img.url} />
        <LinkCard label="Markdown" value={img.markdown} />
        <LinkCard label="HTML" value={img.html} />
        <LinkCard label="BBCode" value={img.bbcode} />
      </div>
    </div>
  )
}
