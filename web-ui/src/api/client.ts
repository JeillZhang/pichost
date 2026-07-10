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
  prefixUrl: '/api/v1',
  hooks: {
    beforeRequest: [
      (request) => {
        const token = localStorage.getItem('access_token')
        if (token) {
          request.headers.set('Authorization', `Bearer ${token}`)
        }
      },
    ],
    afterResponse: [
      async (_request, _options, response) => {
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

export async function login(
  username: string,
  password: string,
): Promise<AuthResponse> {
  return api
    .post('auth/login', { json: { username, password } })
    .json<AuthResponse>()
}

export default api
