import ky from 'ky'

export interface UserInfo {
  id: string
  username: string
  is_admin: boolean
  created_at: string
}

export interface AuthResponse {
  access_token: string
  refresh_token: string
  user: UserInfo
}

const api = ky.create({
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
      ({ response }) => {
        if (response.status === 401) {
          localStorage.removeItem('access_token')
          localStorage.removeItem('refresh_token')
          localStorage.removeItem('user')
          window.location.href = '/login'
        }
      },
    ],
  },
})

export async function register(
  username: string,
  password: string,
): Promise<AuthResponse> {
  return api
    .post('auth/register', { json: { username, password } })
    .json<AuthResponse>()
}

export interface UploadResult {
  id: string
  public_key: string
  original_name: string
  url: string
  markdown: string
  html: string
  bbcode: string
  sha256: string
  file_size: number
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
  created_at: string
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
