import { type ButtonHTMLAttributes, type ReactNode, useCallback } from 'react'

type ButtonVariant = 'primary' | 'danger' | 'ghost' | 'icon'
type ButtonSize = 'sm' | 'md'

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: ButtonVariant
  size?: ButtonSize
  children: ReactNode
}

const variantStyles: Record<ButtonVariant, React.CSSProperties> = {
  primary: { backgroundColor: 'var(--color-accent)', color: 'white' },
  danger: { backgroundColor: 'var(--color-danger)', color: 'white' },
  ghost: { backgroundColor: 'transparent', color: 'var(--color-text-muted)', border: '1px solid var(--color-border)' },
  icon: { backgroundColor: 'transparent', color: 'var(--color-text-muted)' },
}

const hoverStyles: Record<ButtonVariant, React.CSSProperties> = {
  primary: { backgroundColor: 'var(--color-accent-hover)' },
  danger: { backgroundColor: 'var(--color-danger-hover)' },
  ghost: { backgroundColor: 'var(--color-surface)', color: 'var(--color-text-secondary)' },
  icon: { backgroundColor: 'var(--color-surface)', color: 'var(--color-text-secondary)' },
}

const sizeStyles: Record<ButtonSize, React.CSSProperties> = {
  sm: { padding: '0.375rem 0.75rem', fontSize: '0.75rem' },
  md: { padding: '0.5rem 1rem', fontSize: '0.875rem' },
}

export default function Button({
  variant = 'primary',
  size = 'md',
  children,
  style,
  ...props
}: ButtonProps) {
  const onMouseEnter = useCallback(
    (e: React.MouseEvent<HTMLButtonElement>) => {
      if (!props.disabled) Object.assign(e.currentTarget.style, hoverStyles[variant])
      props.onMouseEnter?.(e)
    },
    [variant, props.disabled],
  )

  const onMouseLeave = useCallback(
    (e: React.MouseEvent<HTMLButtonElement>) => {
      if (!props.disabled) Object.assign(e.currentTarget.style, variantStyles[variant])
      props.onMouseLeave?.(e)
    },
    [variant, props.disabled],
  )

  return (
    <button
      {...props}
      style={{
        ...variantStyles[variant],
        ...sizeStyles[size],
        borderRadius: 'var(--radius-md)',
        fontWeight: 500,
        cursor: props.disabled ? 'not-allowed' : 'pointer',
        opacity: props.disabled ? 0.5 : 1,
        display: 'inline-flex',
        alignItems: 'center',
        gap: '0.375rem',
        transition: 'all 0.15s ease',
        ...style,
      }}
      onMouseEnter={onMouseEnter}
      onMouseLeave={onMouseLeave}
    >
      {children}
    </button>
  )
}
