export default function Settings() {
  return (
    <div className="mx-auto max-w-2xl p-4">
      {/* OAuth Accounts */}
      <div className="rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-4 backdrop-blur-sm">
        <h3 className="mb-2 text-sm font-medium text-[var(--color-text-primary)]">
          OAuth Accounts
        </h3>
        <p className="mb-3 text-xs text-[var(--color-text-muted)]">
          Link your GitHub or Google account for one-click login. After registration, use the buttons
          below to connect your account, then sign in with OAuth.
        </p>
        <div className="flex gap-2">
          <a
            href="/api/v1/auth/oauth/github"
            className="flex items-center gap-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-3 py-1.5 text-xs text-[var(--color-text-primary)] backdrop-blur-sm hover:bg-[var(--color-surface)] transition-colors"
          >
            Link GitHub
          </a>
          <a
            href="/api/v1/auth/oauth/google"
            className="flex items-center gap-2 rounded-lg border border-[var(--color-border)] bg-[var(--color-surface-glass)] px-3 py-1.5 text-xs text-[var(--color-text-primary)] backdrop-blur-sm hover:bg-[var(--color-surface)] transition-colors"
          >
            Link Google
          </a>
        </div>
      </div>
    </div>
  )
}
