import { ArrowUpDown } from 'lucide-react'

interface SortDropdownProps {
  sort: string
  order: string
  onSortChange: (sort: string) => void
  onOrderChange: (order: string) => void
}

const SORT_OPTIONS = [
  { value: 'created_at', label: 'Upload Date' },
  { value: 'file_size', label: 'File Size' },
  { value: 'original_name', label: 'Filename' },
]

export default function SortDropdown({
  sort,
  order,
  onSortChange,
  onOrderChange,
}: SortDropdownProps) {
  return (
    <div className="flex items-center gap-2">
      <ArrowUpDown className="h-4 w-4 text-[var(--color-text-muted)]" />
      <select
        value={sort}
        onChange={(e) => onSortChange(e.target.value)}
        className="rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-2 py-2 text-sm text-[var(--color-text-primary)] backdrop-blur-sm focus:border-[var(--color-accent)] focus:outline-none"
      >
        {SORT_OPTIONS.map((opt) => (
          <option key={opt.value} value={opt.value}>
            {opt.label}
          </option>
        ))}
      </select>
      <button
        onClick={() => onOrderChange(order === 'asc' ? 'desc' : 'asc')}
        className="rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-2 py-2 text-sm text-[var(--color-text-primary)] backdrop-blur-sm hover:bg-[var(--color-surface-hover)]"
        aria-label={`Sort ${order === 'asc' ? 'descending' : 'ascending'}`}
      >
        {order === 'asc' ? '↑' : '↓'}
      </button>
    </div>
  )
}
