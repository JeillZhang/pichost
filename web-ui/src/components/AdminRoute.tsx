import { type ReactNode } from 'react'
import { Navigate } from 'react-router-dom'
import { useAuthStore } from '../stores/auth'

interface AdminRouteProps {
  children: ReactNode
}

export default function AdminRoute({ children }: AdminRouteProps) {
  const user = useAuthStore((s) => s.user)
  const hasLoaded = useAuthStore((s) => s.hasLoaded)

  if (!hasLoaded) {
    return (
      <div
        className="flex min-h-screen items-center justify-center"
        style={{ backgroundColor: 'var(--color-bg)', color: 'var(--color-text-muted)' }}
      >
        <div
          className="h-8 w-8 animate-spin rounded-full border-2"
          style={{ borderColor: 'var(--color-border)', borderTopColor: 'var(--color-accent)' }}
        />
      </div>
    )
  }

  if (!user?.is_admin) {
    return <Navigate to="/dashboard" replace />
  }

  return <>{children}</>
}
