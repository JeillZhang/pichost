import { useState } from 'react'
import { Link } from 'lucide-react'

interface UrlUploadInputProps {
  onUpload: (url: string) => Promise<void>
}

export default function UrlUploadInput({ onUpload }: UrlUploadInputProps) {
  const [url, setUrl] = useState('')
  const [loading, setLoading] = useState(false)

  const handleSubmit = async () => {
    const trimmed = url.trim()
    if (!trimmed || loading) return
    setLoading(true)
    try {
      await onUpload(trimmed)
      setUrl('')
    } finally {
      setLoading(false)
    }
  }

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      e.preventDefault()
      handleSubmit()
    }
  }

  return (
    <div className="flex items-center gap-2">
      <div className="relative flex-1">
        <Link
          className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2"
          style={{ color: 'var(--color-text-muted)' }}
        />
        <input
          type="url"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Paste image URL..."
          disabled={loading}
          className="input-field pl-9 text-sm"
          style={{ paddingTop: '0.375rem', paddingBottom: '0.375rem' }}
        />
      </div>
      <button
        onClick={handleSubmit}
        disabled={!url.trim() || loading}
        className="btn-accent shrink-0 px-3 py-1.5 text-sm"
      >
        {loading ? '...' : 'Upload'}
      </button>
    </div>
  )
}
