import ky from 'ky'
import type { KyInstance } from 'ky'

export interface UserInfo {
  id: string
  username: string
  email?: string | null
  is_admin: boolean
  created_at: string
}

export interface AuthResponse {
  access_token: string
  refresh_token: string
  user: UserInfo
}

export interface ImageInfo {
  id: string
  public_key: string
  original_name: string
  url: string
  markdown: string
  html: string
  bbcode: string
  sha256: string
  file_size: number
  mime_type: string
  width: number | null
  height: number | null
  status: string
  thumbnail_url: string | null
  webp_url: string | null
  created_at: string
}

export interface UploadResult extends ImageInfo {}

export async function refreshToken(): Promise<AuthResponse> {
  const refreshToken = localStorage.getItem('refresh_token')
  if (!refreshToken) throw new Error('No refresh token')
  return api.post('auth/refresh', { json: { refresh_token: refreshToken } }).json<AuthResponse>()
}

export async function logout(): Promise<void> {
  await api.post('auth/logout', { json: {} }).json()
}

export async function deleteImage(id: string): Promise<void> {
  await api.delete(`images/${id}`).json()
}

function createApi(): KyInstance {
  return ky.create({
    prefix: '/api/v1',
    hooks: {
      beforeRequest: [
        ({ request }) => {
          const token = localStorage.getItem('access_token')
          if (token) {
            request.headers.set('Authorization', `Bearer ${token}`)
          }
        },
      ],
      afterResponse: [
        async ({ request, response }) => {
          // Skip refresh on auth endpoints to avoid infinite loop
          // when the refresh token itself is expired
          if (response.status === 401 && !request.url.includes('/auth/')) {
            try {
              const { useAuthStore } = await import('../stores/auth')
              const refreshed = await useAuthStore.getState().refresh()
              if (refreshed) {
                const token = localStorage.getItem('access_token')
                const headers = new Headers(request.headers)
                headers.set('Authorization', `Bearer ${token}`)
                return ky.retry({
                  request: new Request(request, { headers }),
                  code: 'TOKEN_REFRESHED',
                })
              }
              useAuthStore.getState().forceLogout()
            } catch {
              localStorage.removeItem('access_token')
              localStorage.removeItem('refresh_token')
              localStorage.removeItem('user')
              window.location.href = '/login'
            }
          }
        },
      ],
    },
  })
}

const api = createApi()

export async function register(
  username: string,
  password: string,
): Promise<AuthResponse> {
  return api
    .post('auth/register', { json: { username, password } })
    .json<AuthResponse>()
}

export async function login(
  username: string,
  password: string,
): Promise<AuthResponse> {
  return api
    .post('auth/login', { json: { username, password } })
    .json<AuthResponse>()
}

export async function uploadImage(file: File): Promise<UploadResult> {
  const formData = new FormData()
  formData.append('file', file)
  return api.post('images', { body: formData }).json<UploadResult>()
}

export async function listImages(): Promise<ImageInfo[]> {
  return api.get('images').json<ImageInfo[]>()
}

export async function getImage(id: string): Promise<ImageInfo> {
  return api.get(`images/${id}`).json<ImageInfo>()
}

export default api
