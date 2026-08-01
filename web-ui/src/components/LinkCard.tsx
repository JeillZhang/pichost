import { useState } from 'react'
import { Copy, Check } from 'lucide-react'
import { toast } from 'sonner'

interface LinkCardProps {
  label: string
  value: string
}

export default function LinkCard({ label, value }: LinkCardProps) {
  const [copied, setCopied] = useState(false)

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(value)
      setCopied(true)
      toast.success(`${label} copied`)
      setTimeout(() => setCopied(false), 2000)
    } catch {
      toast.error('Failed to copy')
    }
  }

  return (
    <div className="glass group rounded-lg p-3 transition-all duration-200">
      <div
        className="mb-1 text-[11px] font-semibold uppercase tracking-wider"
        style={{ color: 'var(--color-text-muted)' }}
      >
        {label}
      </div>
      <div className="flex items-center gap-2">
        <code
          className="max-w-full flex-1 truncate rounded bg-[var(--color-surface)] px-2 py-1 text-sm"
          style={{
            color: 'var(--color-text-secondary)',
            fontFamily: "'SF Mono', 'Fira Code', 'Cascadia Code', monospace",
            fontSize: '0.8125rem',
          }}
        >
          {value}
        </code>
        <button
          onClick={handleCopy}
          className="shrink-0 rounded-lg p-1.5 transition-all duration-200 hover:bg-[var(--color-surface-hover)]"
          style={{ color: 'var(--color-text-muted)' }}
          title={`Copy ${label}`}
        >
          {copied ? (
            <Check className="h-4 w-4" style={{ color: 'var(--color-success)' }} />
          ) : (
            <Copy className="h-4 w-4" />
          )}
        </button>
      </div>
    </div>
  )
}
