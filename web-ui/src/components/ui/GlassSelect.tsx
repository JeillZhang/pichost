import { type CSSProperties, useCallback, useEffect, useRef, useState } from 'react'
import { Check, ChevronDown } from 'lucide-react'

export interface GlassSelectOption<T extends string = string> {
  value: T
  label: string
}

interface GlassSelectProps<T extends string = string> {
  value: T
  onChange: (value: T) => void
  options: readonly GlassSelectOption<T>[]
  placeholder?: string
  disabled?: boolean
  className?: string
  style?: CSSProperties
  ariaLabel?: string
}

export default function GlassSelect<T extends string = string>({
  value,
  onChange,
  options,
  placeholder = 'Select…',
  disabled = false,
  className = '',
  style,
  ariaLabel,
}: GlassSelectProps<T>) {
  const [open, setOpen] = useState(false)
  const [highlightIndex, setHighlightIndex] = useState(0)
  const containerRef = useRef<HTMLDivElement>(null)

  const selected = options.find((o) => o.value === value)

  const close = useCallback(() => setOpen(false), [])

  useEffect(() => {
    if (!open) return
    const current = options.findIndex((o) => o.value === value)
    setHighlightIndex(current >= 0 ? current : 0)
  }, [open, options, value])

  useEffect(() => {
    if (!open) return
    function handlePointerDown(e: MouseEvent) {
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        close()
      }
    }
    function handleKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') {
        e.preventDefault()
        close()
      }
    }
    document.addEventListener('mousedown', handlePointerDown)
    document.addEventListener('keydown', handleKeyDown)
    return () => {
      document.removeEventListener('mousedown', handlePointerDown)
      document.removeEventListener('keydown', handleKeyDown)
    }
  }, [open, close])

  function handleTriggerKeyDown(e: React.KeyboardEvent) {
    if (disabled) return
    if (e.key === 'Enter' || e.key === ' ' || e.key === 'ArrowDown' || e.key === 'ArrowUp') {
      e.preventDefault()
      setOpen(true)
    }
  }

  function handleListKeyDown(e: React.KeyboardEvent) {
    if (!open) return
    if (e.key === 'ArrowDown') {
      e.preventDefault()
      setHighlightIndex((i) => Math.min(i + 1, options.length - 1))
    } else if (e.key === 'ArrowUp') {
      e.preventDefault()
      setHighlightIndex((i) => Math.max(i - 1, 0))
    } else if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault()
      const opt = options[highlightIndex]
      if (opt) {
        onChange(opt.value)
        close()
      }
    }
  }

  return (
    <div
      ref={containerRef}
      className={`relative ${className}`}
      style={style}
      onKeyDown={open ? handleListKeyDown : handleTriggerKeyDown}
    >
      <button
        type="button"
        role="combobox"
        aria-expanded={open}
        aria-label={ariaLabel}
        aria-haspopup="listbox"
        disabled={disabled}
        onClick={() => !disabled && setOpen((prev) => !prev)}
        className="flex w-full items-center justify-between gap-2 rounded-lg border border-[var(--glass-border-base)] bg-[var(--glass-tint-base)]/65 px-3 py-1.5 text-left text-sm text-[var(--color-text-primary)] transition-colors duration-150 focus:outline-none focus:ring-1 focus:ring-[var(--color-accent)] disabled:opacity-50 disabled:cursor-not-allowed"
      >
        <span className={`flex-1 truncate ${selected ? '' : 'text-[var(--color-text-muted)]'}`}>
          {selected ? selected.label : placeholder}
        </span>
        <ChevronDown
          className={`h-4 w-4 shrink-0 transition-transform duration-150 ${
            open ? 'rotate-180' : ''
          }`}
          style={{ color: 'var(--color-text-muted)' }}
        />
      </button>

      {open && !disabled && (
        <div
          role="listbox"
          aria-label={ariaLabel}
          className="glass-elevated absolute left-0 right-0 z-50 max-h-60 overflow-auto rounded-lg py-1"
          style={{ top: 'calc(100% + 0.375rem)', boxShadow: 'var(--glass-shadow)' }}
        >
          {options.map((opt, i) => {
            const isSelected = opt.value === value
            const isHighlighted = i === highlightIndex
            return (
              <button
                key={opt.value}
                type="button"
                role="option"
                aria-selected={isSelected}
                onClick={() => {
                  onChange(opt.value)
                  close()
                }}
                onMouseEnter={() => setHighlightIndex(i)}
                className="flex w-full items-center gap-2.5 px-3.5 py-2 text-left text-sm transition-colors duration-100"
                style={{
                  backgroundColor: isHighlighted ? 'var(--color-surface-hover)' : 'transparent',
                  color: isSelected ? 'var(--color-text-primary)' : 'var(--color-text-secondary)',
                }}
              >
                <span className="flex-1 truncate">{opt.label}</span>
                {isSelected && <Check className="h-4 w-4 shrink-0" style={{ color: 'var(--color-accent)' }} />}
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}
