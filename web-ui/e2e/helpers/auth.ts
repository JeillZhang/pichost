/**
 * Auth setup helpers.
 *
 * IMPORTANT: Playwright runs each spec file in its own worker process, so
 * module-level state does NOT persist across files (and a failed test
 * restarts the worker, wiping it mid-file). Every spec file must therefore
 * call `ensureAuth()` in its own beforeAll and keep tokens file-locally.
 */
import type { APIRequestContext, Page } from '@playwright/test'
import { TEST_ADMIN, TEST_USER } from './fixtures'
import { registerUser, loginUser, createInvite, type AuthResponse } from './api'

export interface AuthContext {
  admin: AuthResponse
  user: AuthResponse
  inviteCode: string
}

async function tryLogin(
  request: APIRequestContext,
  username: string,
  password: string,
): Promise<AuthResponse | null> {
  const res = await request.post('/api/v1/auth/login', { data: { username, password } })
  if (!res.ok()) return null
  return res.json()
}

/**
 * Idempotent: logs in the admin + user if they exist, otherwise registers
 * them (first registration in a fresh DB becomes the auto-admin). Returns
 * fresh tokens plus a fresh invite code. Call once per spec file.
 */
export async function ensureAuth(request: APIRequestContext): Promise<AuthContext> {
  // Admin: login, or register as first user (auto-admin on empty DB)
  let admin = await tryLogin(request, TEST_ADMIN.username, TEST_ADMIN.password)
  if (!admin) {
    admin = await registerUser(request, TEST_ADMIN.username, TEST_ADMIN.password)
  }

  // User: login, or register via a fresh invite code
  let user = await tryLogin(request, TEST_USER.username, TEST_USER.password)
  let inviteCode = ''
  if (!user) {
    const invite = await createInvite(request, admin.access_token, 7)
    inviteCode = invite.code
    user = await registerUser(request, TEST_USER.username, TEST_USER.password, invite.code)
  }

  return { admin, user, inviteCode }
}

/** Seeds a fresh session for the standard E2E user. */
export async function seedUserSession(page: Page, request: APIRequestContext) {
  await seedSession(page, request, TEST_USER.username, TEST_USER.password)
}

/** Seeds a fresh session for the standard E2E admin. */
export async function seedAdminSession(page: Page, request: APIRequestContext) {
  await seedSession(page, request, TEST_ADMIN.username, TEST_ADMIN.password)
}

/** Logs a user in through the browser UI (real form flow). */
export async function loginViaUI(page: Page, username: string, password: string) {
  await page.goto('/login')
  await page.getByLabel(/username/i).fill(username)
  await page.getByLabel(/password/i).fill(password)
  await page.getByRole('button', { name: /sign in/i }).click()
  await page.waitForURL('**/dashboard')
}

/**
 * Logs in fresh and injects the session into the page's localStorage so a
 * spec starts authenticated without driving the login form.
 *
 * A FRESH login per call is required: the ky 401-interceptor refreshes (and
 * thereby ROTATES/revokes) the seeded pair whenever any endpoint 401s — e.g.
 * the wrong-password test burns its tokens — so reusing one shared pair
 * across tests would leave later tests logged out. Fresh pairs also avoid
 * cross-test coupling entirely.
 */
export async function seedSession(
  page: Page,
  request: APIRequestContext,
  username: string,
  password: string,
) {
  const auth = await loginUser(request, username, password)
  await page.goto('/login')
  await page.evaluate(
    ({ access, refresh, user }) => {
      localStorage.setItem('access_token', access)
      localStorage.setItem('refresh_token', refresh)
      localStorage.setItem('user', JSON.stringify(user))
    },
    { access: auth.access_token, refresh: auth.refresh_token, user: auth.user },
  )
}
