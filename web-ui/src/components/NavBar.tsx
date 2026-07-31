import { Link, NavLink, useNavigate } from 'react-router-dom'
import { LogOut, Settings, Shield, User } from 'lucide-react'
import { useAuthStore } from '../stores/auth'
import ThemeToggle from './ThemeToggle'
import DropdownMenu, { type DropdownMenuItem } from './ui/DropdownMenu'

export default function NavBar() {
  const user = useAuthStore((s) => s.user)
  const logout = useAuthStore((s) => s.logout)
  const navigate = useNavigate()

  return (
    <nav
      className="sticky top-0 z-50 border-b backdrop-blur-sm"
      style={{
        backgroundColor: 'var(--glass-bg)',
        borderColor: 'var(--glass-border)',
      }}
    >
      <div className="mx-auto flex max-w-5xl items-center justify-between px-4 py-3">
        {/* Brand */}
        <Link
          to="/dashboard"
          className="text-lg font-bold"
          style={{ color: 'var(--color-text-primary)' }}
        >
          PicHost
        </Link>

        {/* Nav links */}
        <div className="flex items-center gap-4">
          <NavLink
            to="/dashboard"
            className={({ isActive }) =>
              isActive ? '' : 'hover:opacity-75'
            }
            style={({ isActive }) => ({
              color: isActive ? 'var(--color-text-primary)' : 'var(--color-text-secondary)',
            })}
          >
            Dashboard
          </NavLink>
          <NavLink
            to="/gallery"
            className={({ isActive }) =>
              isActive ? '' : 'hover:opacity-75'
            }
            style={({ isActive }) => ({
              color: isActive ? 'var(--color-text-primary)' : 'var(--color-text-secondary)',
            })}
          >
            Gallery
          </NavLink>
          {user?.is_admin && (
            <NavLink
              to="/admin"
              className={({ isActive }) =>
                isActive ? '' : 'hover:opacity-75'
              }
              style={({ isActive }) => ({
                color: isActive ? 'var(--color-text-primary)' : 'var(--color-text-secondary)',
              })}
            >
              Admin
            </NavLink>
          )}
        </div>

        {/* User */}
        <div className="flex items-center gap-3">
          <ThemeToggle />
          <DropdownMenu
            trigger={
              <span
                className="flex max-w-[180px] cursor-pointer items-center gap-2 rounded-lg px-2 py-1.5 text-sm transition-colors"
                style={{ color: 'var(--color-text-secondary)' }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.backgroundColor = 'var(--color-surface)'
                  e.currentTarget.style.color = 'var(--color-text-primary)'
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.backgroundColor = 'transparent'
                  e.currentTarget.style.color = 'var(--color-text-secondary)'
                }}
              >
                <User className="h-4 w-4 shrink-0" />
                <span className="max-w-[120px] truncate">{user?.username}</span>
                {user?.is_admin && (
                  <span
                    className="inline-block rounded px-1.5 py-0.5 text-[10px] font-semibold uppercase leading-none"
                    style={{ backgroundColor: 'var(--color-accent-subtle)', color: 'var(--color-accent)' }}
                  >
                    Admin
                  </span>
                )}
              </span>
            }
            items={(() => {
              const items: DropdownMenuItem[] = [
                { label: 'Settings', icon: <Settings className="h-4 w-4" />, onClick: () => navigate('/settings') },
              ]
              if (user?.is_admin) {
                items.push({ label: 'Admin', icon: <Shield className="h-4 w-4" />, onClick: () => navigate('/admin') })
              }
              items.push({
                label: 'Logout',
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
