import { useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { UserPlus, Loader2, KeyRound } from 'lucide-react'
import { toast } from 'sonner'
import { useAuthStore } from '../stores/auth'

export default function Register() {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const [inviteCode, setInviteCode] = useState('')
  const { register, isLoading, error } = useAuthStore()
  const navigate = useNavigate()

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    await register(username, password, inviteCode || undefined)
    const state = useAuthStore.getState()
    if (state.isAuthenticated) {
      if (state.user?.is_admin) {
        toast.success('Admin account created! You are now the administrator.', { duration: 6000 })
      } else {
        toast.success('Registered!')
      }
      navigate('/dashboard', { replace: true })
    }
  }

  return (
    <div
      className="flex min-h-screen items-center justify-center p-4"
      style={{ backgroundColor: 'var(--color-bg)' }}
    >
      <div className="w-full max-w-sm">
        {/* Logo */}
        <div className="mb-8 text-center">
          <h1
            className="text-4xl font-bold"
            style={{
              background: 'linear-gradient(135deg, #a5b4fc 0%, #c084fc 50%, #a5b4fc 100%)',
              WebkitBackgroundClip: 'text',
              WebkitTextFillColor: 'transparent',
              fontFamily: "'Outfit', system-ui, sans-serif",
            }}
          >
            PicHost
          </h1>
          <p className="mt-1.5 text-sm" style={{ color: 'var(--color-text-muted)' }}>
            Create your account
          </p>
        </div>

        {/* Form card */}
        <form onSubmit={handleSubmit} className="glass-elevated space-y-4 p-6">
          {error && (
            <div
              className="rounded-lg border px-4 py-2.5 text-sm"
              style={{
                backgroundColor: 'var(--color-danger-subtle)',
                borderColor: 'var(--color-danger-border)',
                color: 'var(--color-danger)',
              }}
            >
              {error}
            </div>
          )}

          <div>
            <label
              htmlFor="username"
              className="mb-1.5 block text-xs font-medium"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              Username
            </label>
            <input
              id="username"
              type="text"
              required
              minLength={3}
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              className="input-field"
              placeholder="your username"
            />
          </div>

          <div>
            <label
              htmlFor="password"
              className="mb-1.5 block text-xs font-medium"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              Password
            </label>
            <input
              id="password"
              type="password"
              required
              minLength={8}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              className="input-field"
              placeholder="••••••••"
            />
          </div>

          <div>
            <label
              htmlFor="inviteCode"
              className="mb-1.5 block text-xs font-medium"
              style={{ color: 'var(--color-text-secondary)' }}
            >
              Invite Code
            </label>
            <div className="relative">
              <div className="pointer-events-none absolute inset-y-0 left-0 flex items-center pl-3">
                <KeyRound className="h-4 w-4" style={{ color: 'var(--color-text-muted)' }} />
              </div>
              <input
                id="inviteCode"
                type="text"
                value={inviteCode}
                onChange={(e) => setInviteCode(e.target.value)}
                className="input-field pl-10"
                placeholder="optional invite code"
              />
            </div>
          </div>

          <button type="submit" disabled={isLoading} className="btn-accent w-full">
            {isLoading ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <UserPlus className="h-4 w-4" />
            )}
            Register
          </button>

          <p className="text-center text-sm" style={{ color: 'var(--color-text-muted)' }}>
            Already have an account?{' '}
            <Link
              to="/login"
              style={{ color: 'var(--color-accent)' }}
              className="font-medium hover:underline"
            >
              Sign in
            </Link>
          </p>
        </form>
      </div>
    </div>
  )
}
