import { useState } from 'react'
import { LogOut } from 'lucide-react'
import { useNavigate } from 'react-router-dom'
import { toast } from 'sonner'
import DropZone from '../components/DropZone'
import LinkCard from '../components/LinkCard'
import { uploadImage, listImages, type UploadResult } from '../api/client'
import { useAuthStore } from '../stores/auth'
import { useQuery, useQueryClient } from '@tanstack/react-query'

export default function Dashboard() {
  const [uploadResult, setUploadResult] = useState<UploadResult | null>(null)
  const [isUploading, setIsUploading] = useState(false)
  const { user, logout } = useAuthStore()
  const navigate = useNavigate()
  const queryClient = useQueryClient()

  const { data: images } = useQuery({
    queryKey: ['images'],
    queryFn: listImages,
  })

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

  function handleLogout() {
    logout()
    navigate('/login', { replace: true })
  }

  return (
    <div className="mx-auto max-w-2xl p-4">
      {/* Header */}
      <div className="mb-6 flex items-center justify-between">
        <div>
          <h1 className="text-xl font-bold text-white">PicHost</h1>
          <p className="text-sm text-gray-500">
            Logged in as <span className="text-gray-300">{user?.username}</span>
          </p>
        </div>
        <button
          onClick={handleLogout}
          className="flex items-center gap-1.5 rounded-lg px-3 py-2 text-sm text-gray-400 hover:bg-gray-800 hover:text-gray-200"
        >
          <LogOut className="h-4 w-4" />
          Logout
        </button>
      </div>

      {/* DropZone */}
      <DropZone onUpload={handleUpload} isUploading={isUploading} />

      {/* Upload result links */}
      {uploadResult && (
        <div className="mt-4 space-y-2">
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
          <h2 className="mb-3 text-sm font-medium text-gray-400">Recent</h2>
          <div className="space-y-2">
            {images.map((img) => (
              <div
                key={img.id}
                className="flex items-center gap-3 rounded-lg border border-gray-800 bg-gray-900/30 p-3"
              >
                <img
                  src={img.url}
                  alt={img.original_name}
                  className="h-12 w-12 shrink-0 rounded object-cover"
                />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-sm text-gray-200">
                    {img.original_name}
                  </p>
                  <p className="text-xs text-gray-500">
                    {(img.file_size / 1024).toFixed(1)} KB
                  </p>
                </div>
                <button
                  onClick={() => navigate(`/images/${img.id}`)}
                  className="shrink-0 rounded px-3 py-1.5 text-xs text-gray-400 hover:bg-gray-800 hover:text-gray-200"
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
        <div className="mt-8 text-center text-sm text-gray-600">
          No images yet. Upload one above!
        </div>
      )}
    </div>
  )
}
