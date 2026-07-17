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
    <div className="group rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-3">
      <div className="mb-1 text-xs font-medium text-gray-500">{label}</div>
      <div className="flex items-center gap-2">
        <code className="max-w-full flex-1 truncate text-sm text-gray-300">
          {value}
        </code>
        <button
          onClick={handleCopy}
          className="shrink-0 rounded p-1 text-gray-500 hover:bg-gray-800 hover:text-gray-200"
          title={`Copy ${label}`}
        >
          {copied ? (
            <Check className="h-4 w-4 text-green-400" />
          ) : (
            <Copy className="h-4 w-4" />
          )}
        </button>
      </div>
    </div>
  )
}
