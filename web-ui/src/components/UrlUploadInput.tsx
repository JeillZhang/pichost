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
        <Link className="pointer-events-none absolute left-3 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-[var(--color-text-muted)]" />
        <input
          type="url"
          value={url}
          onChange={(e) => setUrl(e.target.value)}
          onKeyDown={handleKeyDown}
          placeholder="Paste image URL..."
          disabled={loading}
          className="w-full rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] py-1.5 pl-9 pr-3 text-sm text-[var(--color-text-primary)] placeholder-[var(--color-text-muted)] backdrop-blur-sm focus:border-[var(--color-accent)] focus:outline-none disabled:opacity-50"
        />
      </div>
      <button
        onClick={handleSubmit}
        disabled={!url.trim() || loading}
        className="shrink-0 rounded-lg bg-[var(--color-accent)] px-3 py-1.5 text-sm font-medium text-white transition-opacity hover:opacity-90 disabled:cursor-not-allowed disabled:opacity-50"
      >
        {loading ? '...' : 'Upload'}
      </button>
    </div>
  )
}
