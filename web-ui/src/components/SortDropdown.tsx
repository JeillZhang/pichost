import { ArrowUpDown } from 'lucide-react'
import GlassSelect from './ui/GlassSelect'

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
    <div className="flex items-center gap-1.5">
      <ArrowUpDown className="h-4 w-4" style={{ color: 'var(--color-text-muted)' }} />
      <GlassSelect
        value={sort}
        onChange={onSortChange}
        options={SORT_OPTIONS}
        ariaLabel="Sort images"
        className="w-auto min-w-[130px]"
      />
      <button
        onClick={() => onOrderChange(order === 'asc' ? 'desc' : 'asc')}
        className="input-field flex w-10 items-center justify-center px-0 py-2 text-sm transition-colors duration-150 hover:bg-[var(--glass-tint-base)]/85"
        aria-label={`Sort ${order === 'asc' ? 'descending' : 'ascending'}`}
      >
        {order === 'asc' ? '↑' : '↓'}
      </button>
    </div>
  )
}
