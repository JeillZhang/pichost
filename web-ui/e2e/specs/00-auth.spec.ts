/**
 * Authentication flows — registration, invite codes, login, refresh, logout.
 *
 * Self-contained: `ensureAuth` in beforeAll registers the first user
 * (auto-admin) and a second user via invite, then stores tokens file-locally.
 */
import { test, expect } from '@playwright/test'
import { ensureAuth, loginViaUI, type AuthContext } from '../helpers/auth'
import { TEST_ADMIN, TEST_USER, uniqueUsername } from '../helpers/fixtures'
import { API_BASE } from '../helpers/api'

let auth: AuthContext

test.beforeAll(async ({ request }) => {
  auth = await ensureAuth(request)
})

test.describe('auth', () => {
  test('first user is admin; second user registered via invite', async ({ request }) => {
    expect(auth.admin.user.is_admin).toBe(true)
    expect(auth.user.user.is_admin).toBe(false)

    // Admin token really is admin
    const me = await request.get(`${API_BASE}/users/me`, {
      headers: { Authorization: `Bearer ${auth.admin.access_token}` },
    })
    expect(me.ok()).toBeTruthy()
    expect((await me.json()).is_admin).toBe(true)
  })

  test('registration without invite code fails for non-first user', async ({ request }) => {
    const res = await request.post(`${API_BASE}/auth/register`, {
      data: { username: uniqueUsername('noinvite'), password: 'Pass123!' },
    })
    expect(res.status()).toBe(400)
    expect((await res.json()).error).toContain('invite code is required')
  })

  test('registration with invalid invite code fails', async ({ request }) => {
    const res = await request.post(`${API_BASE}/auth/register`, {
      data: { username: uniqueUsername('badinv'), password: 'Pass123!', invite_code: 'bogus-code' },
    })
    expect(res.status()).toBe(400)
    expect((await res.json()).error).toContain('invalid invite code')
  })

  test('registration with consumed invite code fails', async ({ request }) => {
    // auth.inviteCode was consumed by ensureAuth — reusing it must fail
    const res = await request.post(`${API_BASE}/auth/register`, {
      data: {
        username: uniqueUsername('reused'),
        password: 'Pass123!',
        invite_code: auth.inviteCode,
      },
    })
    expect(res.status()).toBe(400)
    expect((await res.json()).error).toContain('already been used')
  })

  test('registration with short password fails', async ({ request }) => {
    const res = await request.post(`${API_BASE}/auth/register`, {
      data: {
        username: uniqueUsername('shortpw'),
        password: '12345',
        invite_code: auth.inviteCode,
      },
    })
    expect(res.status()).toBe(400)
  })

  test('duplicate username registration fails with 409', async ({ request }) => {
    // Need a fresh (unconsumed) invite so the request passes the invite gate
    // and reaches the username-uniqueness check.
    const invite = await request.post(`${API_BASE}/admin/invites`, {
      data: { ttl_days: 7 },
      headers: { Authorization: `Bearer ${auth.admin.access_token}` },
    })
    expect(invite.status()).toBe(200)
    const { code } = await invite.json()

    const res = await request.post(`${API_BASE}/auth/register`, {
      data: { username: TEST_ADMIN.username, password: 'AnotherPass123!', invite_code: code },
    })
    expect(res.status()).toBe(409)
  })

  test('login with wrong password fails', async ({ request }) => {
    const res = await request.post(`${API_BASE}/auth/login`, {
      data: { username: TEST_USER.username, password: 'wrong-password' },
    })
    expect(res.status()).toBe(401)
    expect((await res.json()).error).toContain('invalid username or password')
  })

  test('login with nonexistent user fails', async ({ request }) => {
    const res = await request.post(`${API_BASE}/auth/login`, {
      data: { username: 'nobody-here', password: 'Whatever123!' },
    })
    expect(res.status()).toBe(401)
  })

  test('login via UI redirects to dashboard', async ({ page }) => {
    await loginViaUI(page, TEST_USER.username, TEST_USER.password)
    await expect(page).toHaveURL(/\/dashboard/)
    await expect(page.getByText(/dashboard/i).first()).toBeVisible()
  })

  test('token refresh rotates the pair and revokes the old token', async ({ request }) => {
    // Login for a fresh pair we control
    const login = await request.post(`${API_BASE}/auth/login`, {
      data: { username: TEST_USER.username, password: TEST_USER.password },
    })
    const { refresh_token: rt } = await login.json()

    const res = await request.post(`${API_BASE}/auth/refresh`, {
      data: { refresh_token: rt },
    })
    expect(res.status()).toBe(200)
    const body = await res.json()
    expect(body.access_token).toBeTruthy()
    expect(body.refresh_token).toBeTruthy()
    // The old refresh token is now revoked (rotation)
    const again = await request.post(`${API_BASE}/auth/refresh`, {
      data: { refresh_token: rt },
    })
    expect(again.status()).toBe(401)
  })

  test('logout blacklists the access token', async ({ request }) => {
    // Login to get a fresh pair we can burn
    const login = await request.post(`${API_BASE}/auth/login`, {
      data: { username: TEST_USER.username, password: TEST_USER.password },
    })
    const { access_token: access } = await login.json()

    const logout = await request.post(`${API_BASE}/auth/logout`, {
      headers: { Authorization: `Bearer ${access}` },
    })
    expect(logout.status()).toBe(200)

    // Access token must now be rejected
    const me = await request.get(`${API_BASE}/users/me`, {
      headers: { Authorization: `Bearer ${access}` },
    })
    expect(me.status()).toBe(401)
  })
})
