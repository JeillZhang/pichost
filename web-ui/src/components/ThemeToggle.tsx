import { Sun, Moon, Monitor } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useUiStore } from '../stores/ui'

const icons: Record<string, typeof Sun> = {
  light: Sun,
  dark: Moon,
  system: Monitor,
}

export default function ThemeToggle() {
  const theme = useUiStore((s) => s.theme)
  const toggleTheme = useUiStore((s) => s.toggleTheme)
  const { t } = useTranslation()
  const Icon = icons[theme] || Monitor

  return (
    <button
      onClick={toggleTheme}
      className="rounded-lg p-2 transition-all duration-200 hover:bg-[var(--glass-tint-base)]/65 hover:text-[var(--color-text-secondary)]"
      style={{ color: 'var(--color-text-muted)' }}
      title={t('themeToggle.title', { theme })}
    >
      <Icon className="h-4 w-4" />
    </button>
  )
}
