import { Link, NavLink, useNavigate } from 'react-router-dom'
import { LogOut } from 'lucide-react'
import { useAuthStore } from '../stores/auth'
import ThemeToggle from './ThemeToggle'

export default function NavBar() {
  const user = useAuthStore((s) => s.user)
  const logout = useAuthStore((s) => s.logout)
  const navigate = useNavigate()

  function handleLogout() {
    logout()
    navigate('/login', { replace: true })
  }

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
          <span className="text-sm" style={{ color: 'var(--color-text-muted)' }}>
            Logged in as{' '}
            <span style={{ color: 'var(--color-text-secondary)' }}>
              {user?.username}
            </span>
          </span>
          <ThemeToggle />
          <button
            onClick={handleLogout}
            className="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm"
            style={{
              color: 'var(--color-text-muted)',
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.backgroundColor = 'var(--color-surface)'
              e.currentTarget.style.color = 'var(--color-text-secondary)'
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.backgroundColor = 'transparent'
              e.currentTarget.style.color = 'var(--color-text-muted)'
            }}
          >
            <LogOut className="h-4 w-4" />
            Logout
          </button>
        </div>
      </div>
    </nav>
  )
}
