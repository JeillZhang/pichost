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
    <div className="flex min-h-screen items-center justify-center p-4" style={{ backgroundColor: 'var(--color-bg)' }}>
      <div className="w-full max-w-sm">
        <div className="mb-8 text-center">
          <h1 className="text-4xl font-bold" style={{
            background: 'linear-gradient(135deg, #3b82f6, #8b5cf6)',
            WebkitBackgroundClip: 'text', WebkitTextFillColor: 'transparent',
          }}>
            PicHost
          </h1>
          <p className="mt-1 text-sm" style={{ color: 'var(--color-text-muted)' }}>
            Create your account
          </p>
        </div>
        <form onSubmit={handleSubmit} className="space-y-4 rounded-xl p-6"
          style={{
            backgroundColor: 'var(--glass-bg)', border: '1px solid var(--glass-border)',
            backdropFilter: 'blur(var(--glass-blur))', boxShadow: 'var(--glass-shadow)',
          }}>
          {error && (
            <div className="rounded-lg px-4 py-2 text-sm"
              style={{ backgroundColor: 'var(--color-danger-subtle)', color: 'var(--color-danger)' }}>
              {error}
            </div>
          )}
          <div>
            <label htmlFor="username" className="block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>Username</label>
            <input id="username" type="text" required minLength={3} value={username}
              onChange={e => setUsername(e.target.value)}
              className="mt-1 block w-full rounded-lg px-3 py-2 placeholder-gray-500 focus:outline-none focus:ring-1"
              style={{ backgroundColor: 'var(--color-surface)', border: '1px solid var(--color-border)', color: 'var(--color-text-primary)' }}
              placeholder="your username" />
          </div>
          <div>
            <label htmlFor="password" className="block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>Password</label>
            <input id="password" type="password" required minLength={8} value={password}
              onChange={e => setPassword(e.target.value)}
              className="mt-1 block w-full rounded-lg px-3 py-2 placeholder-gray-500 focus:outline-none focus:ring-1"
              style={{ backgroundColor: 'var(--color-surface)', border: '1px solid var(--color-border)', color: 'var(--color-text-primary)' }}
              placeholder="••••••••" />
          </div>
          <div>
            <label htmlFor="inviteCode" className="block text-sm font-medium" style={{ color: 'var(--color-text-secondary)' }}>Invite Code</label>
            <div className="relative mt-1">
              <div className="pointer-events-none absolute inset-y-0 left-0 flex items-center pl-3">
                <KeyRound className="h-4 w-4" style={{ color: 'var(--color-text-muted)' }} />
              </div>
              <input id="inviteCode" type="text" value={inviteCode}
                onChange={e => setInviteCode(e.target.value)}
                className="block w-full rounded-lg py-2 pl-10 pr-3 placeholder-gray-500 focus:outline-none focus:ring-1"
                style={{ backgroundColor: 'var(--color-surface)', border: '1px solid var(--color-border)', color: 'var(--color-text-primary)' }}
                placeholder="optional invite code" />
            </div>
          </div>
          <button type="submit" disabled={isLoading}
            className="flex w-full items-center justify-center gap-2 rounded-lg px-4 py-2.5 text-sm font-medium text-white disabled:opacity-50"
            style={{ backgroundColor: 'var(--color-accent)' }}>
            {isLoading ? <Loader2 className="h-4 w-4 animate-spin" /> : <UserPlus className="h-4 w-4" />}
            Register
          </button>
          <p className="text-center text-sm" style={{ color: 'var(--color-text-muted)' }}>
            Already have an account?{' '}
            <Link to="/login" style={{ color: 'var(--color-accent)' }} className="hover:opacity-80">Sign in</Link>
          </p>
        </form>
      </div>
    </div>
  )
}
