import { useState, type FormEvent } from 'react'
import { Link, useNavigate } from 'react-router-dom'
import { LogIn, Loader2 } from 'lucide-react'
import { toast } from 'sonner'
import { useAuthStore } from '../stores/auth'

export default function Login() {
  const [username, setUsername] = useState('')
  const [password, setPassword] = useState('')
  const { login, isLoading, error } = useAuthStore()
  const navigate = useNavigate()

  async function handleSubmit(e: FormEvent) {
    e.preventDefault()
    await login(username, password)
    const state = useAuthStore.getState()
    if (state.isAuthenticated) {
      toast.success('Logged in!')
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
            Self-hosted image hosting
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

          <button type="submit" disabled={isLoading} className="btn-accent w-full">
            {isLoading ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <LogIn className="h-4 w-4" />
            )}
            Sign In
          </button>

          <p className="text-center text-sm" style={{ color: 'var(--color-text-muted)' }}>
            Don't have an account?{' '}
            <Link
              to="/register"
              style={{ color: 'var(--color-accent)' }}
              className="font-medium hover:underline"
            >
              Register
            </Link>
          </p>
        </form>
      </div>
    </div>
  )
}
