import ky from 'ky'
import type { KyInstance } from 'ky'

export interface UserInfo {
  id: string
  username: string
  email?: string | null
  is_admin: boolean
  storage_quota: number | null
  created_at: string
}

export interface UserProfile {
  id: string
  username: string
  email: string | null
  storage_backend: string
  storage_prefix: string
  storage_quota: number | null
  is_admin: boolean
  created_at: string
  updated_at: string
}

export interface UpdateProfileRequest {
  username?: string
  email?: string
  storage_backend?: string
}

export interface ChangePasswordRequest {
  current_password: string
  new_password: string
}

export interface UserStats {
  total_images: number
  total_size: number
  backend: string
  storage_quota: number | null
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

export interface UploadResult extends ImageInfo {
  storage_configs?: StorageConfigInfo[]
}

export interface PaginatedListParams {
  page?: number
  per_page?: number
  sort?: 'created_at' | 'file_size' | 'original_name'
  order?: 'asc' | 'desc'
  search?: string
  storage_config_id?: string
}

export interface PaginatedResponse<T> {
  items: T[]
  total: number
  page: number
  per_page: number
  total_pages: number
}

export interface InviteCodeInfo {
  code: string
  created_by: string
  expires_at: number
  used_by: string | null
  created_at: number
}

export interface CreateInviteResponse {
  code: string
  expires_at: number
}

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
  inviteCode?: string,
): Promise<AuthResponse> {
  const body: Record<string, string> = { username, password }
  if (inviteCode) body.invite_code = inviteCode
  return api
    .post('auth/register', { json: body })
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

export async function uploadImage(
  file: File,
  storageConfigIds?: string[],
): Promise<UploadResult> {
  const formData = new FormData()
  formData.append('file', file)
  if (storageConfigIds?.length) {
    formData.append('storage_config_ids', storageConfigIds.join(','))
  }
  return api.post('images', { body: formData }).json<UploadResult>()
}

export async function listImages(
  params: PaginatedListParams = {},
): Promise<PaginatedResponse<ImageInfo>> {
  const searchParams = new URLSearchParams()
  if (params.page) searchParams.set('page', String(params.page))
  if (params.per_page) searchParams.set('per_page', String(params.per_page))
  if (params.sort) searchParams.set('sort', params.sort)
  if (params.order) searchParams.set('order', params.order)
  if (params.search) searchParams.set('search', params.search)
  if (params.storage_config_id) searchParams.set('storage_config_id', params.storage_config_id)
  const qs = searchParams.toString()
  return api.get(`images${qs ? `?${qs}` : ''}`).json<PaginatedResponse<ImageInfo>>()
}

export async function getImage(id: string): Promise<ImageInfo> {
  return api.get(`images/${id}`).json<ImageInfo>()
}

export async function getUserStats(): Promise<UserStats> {
  return api.get('users/me/stats').json<UserStats>()
}

export async function getUserMe(): Promise<UserProfile> {
  return api.get('users/me').json<UserProfile>()
}

export async function updateUserMe(body: UpdateProfileRequest): Promise<UserProfile> {
  return api.patch('users/me', { json: body }).json<UserProfile>()
}

export async function changePassword(body: ChangePasswordRequest): Promise<{ message: string }> {
  return api.post('users/me/password', { json: body }).json<{ message: string }>()
}

export async function createInviteCode(ttlDays?: number): Promise<CreateInviteResponse> {
  return api
    .post('admin/invites', { json: { ttl_days: ttlDays ?? 7 } })
    .json<CreateInviteResponse>()
}

export async function listInviteCodes(): Promise<InviteCodeInfo[]> {
  return api.get('admin/invites').json<InviteCodeInfo[]>()
}

export interface BatchDeleteResult {
  message: string
  deleted: number
  failed: number
}

export interface UserStorageConfig {
  id: string
  name: string
  provider: 'github' | 'gitcode' | 'local'
  repo: string
  branch: string
  path_prefix: string | null
  is_default: boolean
  token_masked: string
  created_at: string
  updated_at: string
}

export interface CreateStorageConfigRequest {
  name: string
  provider: 'github' | 'gitcode'
  token: string
  repo: string
  branch?: string
  path_prefix?: string
  is_default?: boolean
}

export interface UpdateStorageConfigRequest {
  name?: string
  token?: string
  repo?: string
  branch?: string
  path_prefix?: string
}

export interface StorageConfigInfo {
  id: string
  name: string
  provider: string
}

export async function batchDeleteImages(ids: string[]): Promise<BatchDeleteResult> {
  return api.post('images/batch-delete', { json: { ids } }).json<BatchDeleteResult>()
}

export async function listStorageConfigs(): Promise<UserStorageConfig[]> {
  return api.get('users/me/storage-configs').json<UserStorageConfig[]>()
}

export async function createStorageConfig(
  data: CreateStorageConfigRequest,
): Promise<UserStorageConfig> {
  return api.post('users/me/storage-configs', { json: data }).json<UserStorageConfig>()
}

export async function updateStorageConfig(
  id: string,
  data: UpdateStorageConfigRequest,
): Promise<UserStorageConfig> {
  return api.patch(`users/me/storage-configs/${id}`, { json: data }).json<UserStorageConfig>()
}

export async function deleteStorageConfig(id: string): Promise<void> {
  return api.delete(`users/me/storage-configs/${id}`).json()
}

export async function setDefaultStorageConfig(id: string): Promise<void> {
  return api.post(`users/me/storage-configs/${id}/default`).json()
}

export default api
