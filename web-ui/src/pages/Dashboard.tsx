import { useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Shield } from 'lucide-react'
import { toast } from 'sonner'
import { useAuthStore } from '../stores/auth'
import DropZone from '../components/DropZone'
import LinkCard from '../components/LinkCard'
import { uploadImage, listImages, type UploadResult } from '../api/client'
import { useQuery, useQueryClient } from '@tanstack/react-query'

export default function Dashboard() {
  const user = useAuthStore((s) => s.user)
  const [uploadResult, setUploadResult] = useState<UploadResult | null>(null)
  const [isUploading, setIsUploading] = useState(false)
  const navigate = useNavigate()
  const queryClient = useQueryClient()

  const { data } = useQuery({
    queryKey: ['images'],
    queryFn: () => listImages(),
  })
  const images = data?.items

  async function handleUpload(file: File) {
    setIsUploading(true)
    setUploadResult(null)
    try {
      const result = await uploadImage(file)
      setUploadResult(result)
      toast.success('Uploaded!')
      queryClient.invalidateQueries({ queryKey: ['images'] })
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : 'Upload failed'
      toast.error(msg)
    } finally {
      setIsUploading(false)
    }
  }

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

      {/* DropZone */}
      <DropZone onUpload={handleUpload} isUploading={isUploading} />

      {/* Upload result links */}
      {uploadResult && (
        <div className="mt-4 space-y-2">
          {uploadResult.status && (
            <p className="flex items-center gap-2 text-sm text-[var(--color-text-secondary)]">
              Status:{' '}
              <span className="rounded border border-green-700 bg-green-900/50 px-2 py-0.5 text-xs font-medium text-green-400">
                {uploadResult.status}
              </span>
            </p>
          )}
          {uploadResult.url && (
            <LinkCard label="URL" value={uploadResult.url} />
          )}
          {uploadResult.markdown && (
            <LinkCard label="Markdown" value={uploadResult.markdown} />
          )}
          {uploadResult.html && (
            <LinkCard label="HTML" value={uploadResult.html} />
          )}
          {uploadResult.bbcode && (
            <LinkCard label="BBCode" value={uploadResult.bbcode} />
          )}
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
                className="flex items-center gap-3 rounded-lg border border-[var(--glass-border)] bg-[var(--glass-bg)] p-3"
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
      {images && images.length === 0 && !uploadResult && (
        <div className="mt-8 text-center text-sm text-[var(--color-text-muted)]">
          No images yet. Upload one above!
        </div>
      )}
    </div>
  )
}
