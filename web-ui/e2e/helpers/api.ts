/**
 * API helpers built on Playwright's APIRequestContext — used for fast,
 * reliable test setup (register users, create invites, upload images)
 * without driving the browser UI.
 */
import type { APIRequestContext } from '@playwright/test'

export const API_BASE = '/api/v1'

export interface AuthResponse {
  access_token: string
  refresh_token: string
  user: { id: string; username: string; is_admin: boolean }
}

export interface UploadResult {
  id: string
  public_key: string
  original_name: string
  url: string
  status: string
}

export async function registerUser(
  request: APIRequestContext,
  username: string,
  password: string,
  inviteCode?: string,
): Promise<AuthResponse> {
  const body: Record<string, string> = { username, password }
  if (inviteCode) body.invite_code = inviteCode
  const res = await request.post(`${API_BASE}/auth/register`, { data: body })
  if (!res.ok()) {
    throw new Error(`register failed: ${res.status()} ${await res.text()}`)
  }
  return res.json()
}

export async function loginUser(
  request: APIRequestContext,
  username: string,
  password: string,
): Promise<AuthResponse> {
  const res = await request.post(`${API_BASE}/auth/login`, { data: { username, password } })
  if (!res.ok()) {
    throw new Error(`login failed: ${res.status()} ${await res.text()}`)
  }
  return res.json()
}

export async function createInvite(
  request: APIRequestContext,
  adminToken: string,
  ttlDays = 7,
): Promise<{ code: string; expires_at: number }> {
  const res = await request.post(`${API_BASE}/admin/invites`, {
    data: { ttl_days: ttlDays },
    headers: { Authorization: `Bearer ${adminToken}` },
  })
  if (!res.ok()) {
    throw new Error(`create invite failed: ${res.status()} ${await res.text()}`)
  }
  return res.json()
}

export async function uploadFile(
  request: APIRequestContext,
  token: string,
  filePath: string,
  mimeType = 'image/png',
): Promise<UploadResult> {
  const fs = await import('node:fs')
  const res = await request.post(`${API_BASE}/images`, {
    headers: { Authorization: `Bearer ${token}` },
    multipart: {
      file: { name: filePath.split('/').pop()!, mimeType, buffer: fs.readFileSync(filePath) },
    },
  })
  if (!res.ok()) {
    throw new Error(`upload failed: ${res.status()} ${await res.text()}`)
  }
  const results = (await res.json()) as UploadResult[]
  return results[0]
}

export async function createCategory(
  request: APIRequestContext,
  token: string,
  name: string,
  parentId?: string,
): Promise<{ id: string }> {
  const body: Record<string, string> = { name }
  if (parentId) body.parent_id = parentId
  const res = await request.post(`${API_BASE}/categories`, {
    data: body,
    headers: { Authorization: `Bearer ${token}` },
  })
  if (!res.ok()) {
    throw new Error(`create category failed: ${res.status()} ${await res.text()}`)
  }
  return res.json()
}
