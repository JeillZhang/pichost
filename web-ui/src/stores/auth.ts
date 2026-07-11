import { create } from 'zustand'
import type { UserInfo } from '../api/client'
import { login as apiLogin, register as apiRegister } from '../api/client'

interface AuthState {
  user: UserInfo | null
  accessToken: string | null
  refreshToken: string | null
  isAuthenticated: boolean
  hasLoaded: boolean
  isLoading: boolean
  error: string | null

  login: (username: string, password: string) => Promise<void>
  register: (username: string, password: string) => Promise<void>
  logout: () => void
  loadFromStorage: () => void
  clearError: () => void
}

export const useAuthStore = create<AuthState>((set) => ({
  user: null,
  accessToken: null,
  refreshToken: null,
  isAuthenticated: false,
  hasLoaded: false,
  isLoading: false,
  error: null,

  login: async (username, password) => {
    set({ isLoading: true, error: null })
    try {
      const res = await apiLogin(username, password)
      localStorage.setItem('access_token', res.access_token)
      localStorage.setItem('refresh_token', res.refresh_token)
      localStorage.setItem('user', JSON.stringify(res.user))
      set({
        user: res.user,
        accessToken: res.access_token,
        refreshToken: res.refresh_token,
        isAuthenticated: true,
        isLoading: false,
      })
    } catch (e: unknown) {
      const message =
        e instanceof Error ? e.message : 'Login failed'
      set({ error: message, isLoading: false })
    }
  },

  register: async (username, password) => {
    set({ isLoading: true, error: null })
    try {
      const res = await apiRegister(username, password)
      localStorage.setItem('access_token', res.access_token)
      localStorage.setItem('refresh_token', res.refresh_token)
      localStorage.setItem('user', JSON.stringify(res.user))
      set({
        user: res.user,
        accessToken: res.access_token,
        refreshToken: res.refresh_token,
        isAuthenticated: true,
        isLoading: false,
      })
    } catch (e: unknown) {
      const message =
        e instanceof Error ? e.message : 'Registration failed'
      set({ error: message, isLoading: false })
    }
  },

  logout: () => {
    localStorage.removeItem('access_token')
    localStorage.removeItem('refresh_token')
    localStorage.removeItem('user')
    set({
      user: null,
      accessToken: null,
      refreshToken: null,
      isAuthenticated: false,
    })
  },

  loadFromStorage: () => {
    const token = localStorage.getItem('access_token')
    const userStr = localStorage.getItem('user')
    if (token && userStr) {
      try {
        const user = JSON.parse(userStr) as UserInfo
        set({
          user,
          accessToken: token,
          refreshToken: localStorage.getItem('refresh_token'),
          isAuthenticated: true,
          hasLoaded: true,
        })
        return
      } catch {
        localStorage.removeItem('access_token')
        localStorage.removeItem('refresh_token')
        localStorage.removeItem('user')
      }
    }
    set({ hasLoaded: true, isAuthenticated: false })
  },

  clearError: () => set({ error: null }),
}))

export const useAuthLoaded = () => useAuthStore((s) => s.hasLoaded)
