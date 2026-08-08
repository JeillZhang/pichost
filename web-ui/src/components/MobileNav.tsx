import { NavLink, useNavigate } from 'react-router-dom'
import { LogOut, Settings, Shield, X } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useAuthStore } from '../stores/auth'
import useOverlay from '../hooks/useOverlay'
import ThemeToggle from './ThemeToggle'
import LanguageSwitcher from './LanguageSwitcher'

const linkBase =
  'relative block rounded-md px-3 py-2.5 text-sm font-medium transition-colors duration-200'
const linkActive = 'bg-[var(--color-accent-subtle)] text-[var(--color-accent)]'
const linkInactive =
  'text-[var(--color-text-secondary)] hover:bg-[var(--color-surface)] hover:text-[var(--color-text-primary)]'

interface MobileNavProps {
  open: boolean
  onClose: () => void
}

export default function MobileNav({ open, onClose }: MobileNavProps) {
  const user = useAuthStore((s) => s.user)
  const logout = useAuthStore((s) => s.logout)
  const navigate = useNavigate()
  const { t } = useTranslation()
  const { overlayProps } = useOverlay(onClose)

  if (!open) return null

  const navLink = (to: string, label: string) => (
    <NavLink
      to={to}
      onClick={onClose}
      className={({ isActive }) => `${linkBase} ${isActive ? linkActive : linkInactive}`}
    >
      {label}
    </NavLink>
  )

  return (
    <>
      <div
        {...overlayProps}
        data-testid="mobile-nav-overlay"
        className="fixed inset-0 z-30 bg-black/30 backdrop-blur-sm"
      />
      <div
        className="glass-nav fixed inset-x-0 top-14 z-40 border-t px-4 pb-4 pt-2"
        style={{ borderColor: 'var(--color-border)' }}
      >
        <div className="flex items-center justify-between">
          <span
            className="text-xs font-medium uppercase tracking-wider"
            style={{ color: 'var(--color-text-muted)' }}
          >
            {user?.username}
          </span>
          <button
            onClick={onClose}
            aria-label={t('modal.close')}
            className="rounded p-1"
            style={{ color: 'var(--color-text-muted)' }}
          >
            <X className="h-5 w-5" />
          </button>
        </div>
        <div className="mt-2 flex flex-col gap-0.5">
          {navLink('/dashboard', t('nav.dashboard'))}
          {navLink('/gallery', t('nav.gallery'))}
          {user?.is_admin && navLink('/admin', t('nav.admin'))}
        </div>
        <div
          className="mt-3 flex items-center justify-between border-t pt-3"
          style={{ borderColor: 'var(--color-border)' }}
        >
          <div className="flex items-center gap-2">
            <ThemeToggle />
            <LanguageSwitcher />
          </div>
          <div className="flex items-center gap-1">
            <button
              onClick={() => { onClose(); navigate('/settings') }}
              className="flex items-center gap-1.5 rounded-md px-2.5 py-2 text-sm"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              <Settings className="h-4 w-4" /> {t('nav.settings')}
            </button>
            {user?.is_admin && (
              <button
                onClick={() => { onClose(); navigate('/admin') }}
                className="flex items-center gap-1.5 rounded-md px-2.5 py-2 text-sm"
                style={{ color: 'var(--color-text-secondary)' }}
              >
                <Shield className="h-4 w-4" /> {t('nav.admin')}
              </button>
            )}
            <button
              onClick={() => { logout(); navigate('/login', { replace: true }) }}
              className="flex items-center gap-1.5 rounded-md px-2.5 py-2 text-sm"
              style={{ color: 'var(--color-danger)' }}
            >
              <LogOut className="h-4 w-4" /> {t('nav.logout')}
            </button>
          </div>
        </div>
      </div>
    </>
  )
}
