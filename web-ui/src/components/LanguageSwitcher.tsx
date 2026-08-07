import { Check, Globe } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { getCurrentLocale } from '../i18n'
import DropdownMenu, { type DropdownMenuItem } from './ui/DropdownMenu'

export default function LanguageSwitcher() {
  const { t, i18n } = useTranslation()
  const current = getCurrentLocale()

  const items: DropdownMenuItem[] = [
    {
      label: t('languageSwitcher.english'),
      icon: current === 'en' ? <Check className="h-4 w-4" /> : undefined,
      onClick: () => {
        void i18n.changeLanguage('en')
      },
    },
    {
      label: t('languageSwitcher.chinese'),
      icon: current === 'zh-CN' ? <Check className="h-4 w-4" /> : undefined,
      onClick: () => {
        void i18n.changeLanguage('zh-CN')
      },
    },
  ]

  return (
    <DropdownMenu
      align="right"
      trigger={
        <span
          className="cursor-pointer rounded-lg p-2 transition-all duration-200 hover:bg-[var(--glass-tint-base)]/65 hover:text-[var(--color-text-secondary)]"
          style={{ color: 'var(--color-text-muted)' }}
        >
          <Globe className="h-4 w-4" />
        </span>
      }
      items={items}
    />
  )
}
