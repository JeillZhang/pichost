import { Link, NavLink, useNavigate } from 'react-router-dom'
import { LogOut } from 'lucide-react'
import { useAuthStore } from '../stores/auth'

export default function NavBar() {
  const user = useAuthStore((s) => s.user)
  const logout = useAuthStore((s) => s.logout)
  const navigate = useNavigate()

  function handleLogout() {
    logout()
    navigate('/login', { replace: true })
  }

  return (
    <nav className="sticky top-0 z-50 border-b border-gray-800 bg-gray-950/80 backdrop-blur-sm">
      <div className="mx-auto flex max-w-5xl items-center justify-between px-4 py-3">
        {/* Brand */}
        <Link to="/dashboard" className="text-lg font-bold text-white">
          PicHost
        </Link>

        {/* Nav links */}
        <div className="flex items-center gap-4">
          <NavLink
            to="/dashboard"
            className={({ isActive }) =>
              isActive ? 'text-white' : 'text-gray-400 hover:text-gray-200'
            }
          >
            Dashboard
          </NavLink>
          <NavLink
            to="/gallery"
            className={({ isActive }) =>
              isActive ? 'text-white' : 'text-gray-400 hover:text-gray-200'
            }
          >
            Gallery
          </NavLink>
        </div>

        {/* User */}
        <div className="flex items-center gap-3">
          <span className="text-sm text-gray-500">
            Logged in as{' '}
            <span className="text-gray-300">{user?.username}</span>
          </span>
          <button
            onClick={handleLogout}
            className="flex items-center gap-1.5 rounded-lg px-3 py-1.5 text-sm text-gray-400 hover:bg-gray-800 hover:text-gray-200"
          >
            <LogOut className="h-4 w-4" />
            Logout
          </button>
        </div>
      </div>
    </nav>
  )
}
