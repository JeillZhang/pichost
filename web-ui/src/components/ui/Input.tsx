import { type InputHTMLAttributes } from 'react'

interface InputProps extends InputHTMLAttributes<HTMLInputElement> {
  label?: string
}

export default function Input({ label, id, style, ...props }: InputProps) {
  return (
    <div>
      {label && (
        <label
          htmlFor={id}
          className="mb-1 block text-sm font-medium"
          style={{ color: 'var(--color-text-secondary)' }}
        >
          {label}
        </label>
      )}
      <input
        id={id}
        {...props}
        className="block w-full rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1"
        style={{
          backgroundColor: 'color-mix(in oklch, var(--glass-tint-base) calc(var(--glass-layer-card-opacity) * 100%), transparent)',
          border: '1px solid var(--glass-border-base)',
          color: 'var(--color-text-primary)',
          ...style,
        }}
      />
    </div>
  )
}
