import { create } from 'zustand'

type Theme = 'light' | 'dark' | 'system'

interface UiState {
  theme: Theme
  setTheme: (theme: Theme) => void
  toggleTheme: () => void
}

function getSystemTheme(): 'light' | 'dark' {
  if (typeof window === 'undefined') return 'dark'
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light'
}

function resolveAndApply(theme: Theme) {
  const resolved = theme === 'system' ? getSystemTheme() : theme
  const root = document.documentElement
  if (resolved === 'dark') {
    root.classList.add('dark')
    root.classList.remove('light')
  } else {
    root.classList.remove('dark')
    root.classList.add('light')
  }
}

// Read initial theme from localStorage before first render
const stored = (typeof localStorage !== 'undefined'
  ? (localStorage.getItem('pichost-theme') as Theme | null)
  : null) ?? 'system'

if (typeof document !== 'undefined') {
  resolveAndApply(stored)
}

export const useUiStore = create<UiState>((set, get) => ({
  theme: stored,

  setTheme: (theme: Theme) => {
    localStorage.setItem('pichost-theme', theme)
    resolveAndApply(theme)
    set({ theme })
  },

  toggleTheme: () => {
    const { theme, setTheme } = get()
    const next: Theme = theme === 'light' ? 'dark' : theme === 'dark' ? 'system' : 'light'
    setTheme(next)
  },
}))

// Listen for system theme changes when in 'system' mode
if (typeof window !== 'undefined') {
  window.matchMedia('(prefers-color-scheme: dark)').addEventListener('change', () => {
    const state = useUiStore.getState()
    if (state.theme === 'system') {
      resolveAndApply('system')
    }
  })
}
