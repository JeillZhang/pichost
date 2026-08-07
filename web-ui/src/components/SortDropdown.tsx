import { ArrowUpDown } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import GlassSelect from './ui/GlassSelect'

interface SortDropdownProps {
  sort: string
  order: string
  onSortChange: (sort: string) => void
  onOrderChange: (order: string) => void
}

export default function SortDropdown({
  sort,
  order,
  onSortChange,
  onOrderChange,
}: SortDropdownProps) {
  const { t } = useTranslation()
  const sortOptions = [
    { value: 'created_at', label: t('sort.uploadDate') },
    { value: 'file_size', label: t('sort.fileSize') },
    { value: 'original_name', label: t('sort.filename') },
  ]
  return (
    <div className="flex items-center gap-1.5">
      <ArrowUpDown className="h-4 w-4" style={{ color: 'var(--color-text-muted)' }} />
      <GlassSelect
        value={sort}
        onChange={onSortChange}
        options={sortOptions}
        ariaLabel={t('sort.sortAria')}
        className="w-auto min-w-[130px]"
      />
      <button
        onClick={() => onOrderChange(order === 'asc' ? 'desc' : 'asc')}
        className="input-field flex w-10 items-center justify-center px-0 py-2 text-sm transition-colors duration-150 hover:bg-[var(--glass-tint-base)]/85"
        aria-label={t('sort.orderAria', {
          order: t(order === 'asc' ? 'sort.descending' : 'sort.ascending'),
        })}
      >
        {order === 'asc' ? '↑' : '↓'}
      </button>
    </div>
  )
}
