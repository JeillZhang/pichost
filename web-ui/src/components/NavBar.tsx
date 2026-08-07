import { Link, NavLink, useNavigate } from 'react-router-dom'
import { LogOut, Settings, Shield, User } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { useAuthStore } from '../stores/auth'
import ThemeToggle from './ThemeToggle'
import LanguageSwitcher from './LanguageSwitcher'
import DropdownMenu, { type DropdownMenuItem } from './ui/DropdownMenu'

const linkBase =
  'relative text-sm font-medium transition-colors duration-200 px-1 py-0.5'
const linkActive = 'text-[var(--color-text-primary)]'
const linkInactive = 'text-[var(--color-text-secondary)] hover:text-[var(--color-text-primary)]'

export default function NavBar() {
  const user = useAuthStore((s) => s.user)
  const logout = useAuthStore((s) => s.logout)
  const navigate = useNavigate()
  const { t } = useTranslation()

  return (
    <nav className="glass-nav sticky top-0 z-50">
      <div className="mx-auto flex max-w-5xl items-center justify-between px-5 py-3">
        {/* Brand */}
        <Link to="/dashboard" className="group flex items-center gap-2">
          <span
            className="bg-gradient-to-br from-cyan-400 via-teal-400 to-cyan-400 bg-clip-text text-xl font-bold text-transparent"
            style={{ fontFamily: "'Outfit', system-ui, sans-serif" }}
          >
            PicHost
          </span>
        </Link>

        {/* Nav links */}
        <div className="flex items-center gap-1">
          <NavLink
            to="/dashboard"
            className={({ isActive }) =>
              `${linkBase} ${isActive ? linkActive : linkInactive}`
            }
          >
            {t('nav.dashboard')}
          </NavLink>
          <NavLink
            to="/gallery"
            className={({ isActive }) =>
              `${linkBase} ${isActive ? linkActive : linkInactive}`
            }
          >
            {t('nav.gallery')}
          </NavLink>
          {user?.is_admin && (
            <NavLink
              to="/admin"
              className={({ isActive }) =>
                `${linkBase} ${isActive ? linkActive : linkInactive}`
              }
            >
              {t('nav.admin')}
            </NavLink>
          )}
        </div>

        {/* User */}
        <div className="flex items-center gap-3">
          <ThemeToggle />
          <LanguageSwitcher />
          <DropdownMenu
            trigger={
              <span
                className="flex max-w-[180px] cursor-pointer items-center gap-2 rounded-lg border border-[var(--glass-border-base)] bg-[var(--glass-tint-base)]/65 px-2.5 py-1.5 text-sm backdrop-blur-sm transition-all duration-200 hover:border-[var(--glass-border-strong)] hover:bg-[var(--glass-tint-base)]/90"
                style={{ color: 'var(--color-text-secondary)' }}
              >
                <User className="h-4 w-4 shrink-0" />
                <span className="max-w-[120px] truncate">{user?.username}</span>
                {user?.is_admin && (
                  <span
                    className="badge"
                    style={{
                      backgroundColor: 'var(--color-accent-subtle)',
                      color: 'var(--color-accent)',
                      borderColor: 'var(--color-accent-strong)',
                    }}
                  >
                    {t('nav.adminBadge')}
                  </span>
                )}
              </span>
            }
            items={(() => {
              const items: DropdownMenuItem[] = [
                { label: t('nav.settings'), icon: <Settings className="h-4 w-4" />, onClick: () => navigate('/settings') },
              ]
              if (user?.is_admin) {
                items.push({ label: t('nav.admin'), icon: <Shield className="h-4 w-4" />, onClick: () => navigate('/admin') })
              }
              items.push({
                label: t('nav.logout'),
                icon: <LogOut className="h-4 w-4" />,
                onClick: () => {
                  logout()
                  navigate('/login', { replace: true })
                },
                danger: true,
              })
              return items
            })()}
          />
        </div>
      </div>
    </nav>
  )
}
