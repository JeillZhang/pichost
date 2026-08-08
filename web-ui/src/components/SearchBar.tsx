import { useState, useEffect, useRef } from 'react'
import { Search, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'

interface SearchBarProps {
  value: string
  onChange: (value: string) => void
  placeholder?: string
}

export default function SearchBar({
  value,
  onChange,
  placeholder,
}: SearchBarProps) {
  const { t } = useTranslation()
  const [localValue, setLocalValue] = useState(value)
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const placeholderText = placeholder ?? t('search.placeholder')

  useEffect(() => {
    setLocalValue(value)
  }, [value])

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current)
    }
  }, [])

  const handleChange = (next: string) => {
    setLocalValue(next)
    if (timerRef.current) clearTimeout(timerRef.current)
    timerRef.current = setTimeout(() => {
      onChange(next)
    }, 300)
  }

  const handleClear = () => {
    setLocalValue('')
    if (timerRef.current) clearTimeout(timerRef.current)
    onChange('')
  }

  return (
    <div className="relative">
      <Search
        className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2"
        style={{ color: 'var(--color-text-muted)' }}
      />
      <input
        type="text"
        value={localValue}
        onChange={(e) => handleChange(e.target.value)}
        placeholder={placeholderText}
        className="input-field pl-10 pr-8"
        style={{ paddingTop: '0.5rem', paddingBottom: '0.5rem' }}
      />
      {localValue && (
        <button
          onClick={handleClear}
          className="absolute right-2.5 top-1/2 -translate-y-1/2 rounded-md p-0.5 transition-colors duration-150 hover:text-[var(--color-text-primary)]"
          style={{ color: 'var(--color-text-muted)' }}
          aria-label={t('search.clearAria')}
        >
          <X className="h-4 w-4" />
        </button>
      )}
    </div>
  )
}
