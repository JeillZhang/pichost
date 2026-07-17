import { useNavigate } from 'react-router-dom'
import { useQuery } from '@tanstack/react-query'
import { listImages } from '../api/client'

export default function Gallery() {
  const navigate = useNavigate()
  const { data: images, isLoading } = useQuery({
    queryKey: ['images'],
    queryFn: listImages,
  })

  if (isLoading) {
    return (
      <div className="flex min-h-screen items-center justify-center text-[var(--color-text-muted)]">
        Loading…
      </div>
    )
  }

  if (!images || images.length === 0) {
    return (
      <div className="flex min-h-screen items-center justify-center text-[var(--color-text-muted)]">
        No images yet.
      </div>
    )
  }

  return (
    <div className="mx-auto max-w-5xl p-4">
      <h1 className="mb-4 text-lg font-bold text-[var(--color-text-primary)]">Gallery</h1>
      <div className="grid grid-cols-2 gap-3 sm:grid-cols-3 md:grid-cols-4">
        {images.map((img) => (
          <button
            key={img.id}
            onClick={() => navigate(`/images/${img.id}`)}
            className="group relative aspect-square overflow-hidden rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] backdrop-blur-sm"
          >
            <img
              src={img.url}
              alt={img.original_name}
              className="h-full w-full object-cover transition-transform group-hover:scale-105"
              loading="lazy"
            />
            <div className="absolute inset-x-0 bottom-0 bg-gradient-to-t from-black/80 to-transparent p-2">
              <p className="truncate text-xs text-[var(--color-text-secondary)]">
                {img.original_name}
              </p>
            </div>
          </button>
        ))}
      </div>
    </div>
  )
}
