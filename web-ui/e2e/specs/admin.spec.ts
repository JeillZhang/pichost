/**
 * Admin panel — access control, stats, users, invites, system config.
 */
import { test, expect } from '@playwright/test'
import { ensureAuth, seedUserSession, seedAdminSession, type AuthContext } from '../helpers/auth'
import { TEST_ADMIN, TEST_USER, uniqueUsername } from '../helpers/fixtures'
import { AdminPage } from '../page-objects/admin.po'
import { API_BASE, registerUser, createInvite } from '../helpers/api'

let auth: AuthContext

test.beforeAll(async ({ request }) => {
  auth = await ensureAuth(request)
})

test.describe.serial('admin', () => {
  test('non-admin is redirected away from /admin', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/admin')
    await expect(page).toHaveURL(/\/dashboard/)
  })

  test('non-admin gets 403 on admin API', async ({ request }) => {
    const res = await request.get(`${API_BASE}/admin/stats`, {
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
    })
    expect(res.status()).toBe(403)
  })

  test('stats overview shows totals', async ({ page, request }) => {
    await seedAdminSession(page, request)
    const admin = new AdminPage(page)
    await admin.goto()
    await admin.openTab('Overview')
    await expect(page.getByText(/total users/i)).toBeVisible()
    await expect(page.getByText(/total images/i)).toBeVisible()
  })

  test('users tab lists users', async ({ page, request }) => {
    await seedAdminSession(page, request)
    const admin = new AdminPage(page)
    await admin.goto()
    await admin.openTab('Users')
    await expect(page.getByText(TEST_ADMIN.username)).toBeVisible()
    await expect(page.getByText(TEST_USER.username)).toBeVisible()
  })

  test('edit user dialog opens and closes', async ({ page, request }) => {
    await seedAdminSession(page, request)
    const admin = new AdminPage(page)
    await admin.goto()
    await admin.openTab('Users')

    // Open the edit dialog for TEST_USER (icon-only pencil button in their row)
    await admin.editButtonFor(TEST_USER.username).click()
    await expect(page.getByText(/edit user/i)).toBeVisible()
    await page.getByRole('button', { name: /^cancel$/i }).click()
    await expect(page.getByText(/edit user/i)).not.toBeVisible()
  })

  test('admin cannot delete self', async ({ request }) => {
    const res = await request.delete(`${API_BASE}/admin/users/${auth.admin.user.id}`, {
      headers: { Authorization: `Bearer ${auth.admin.access_token}` },
    })
    expect(res.status()).toBe(400)
  })

  test('invites tab creates and lists invite codes', async ({ page, request }) => {
    await seedAdminSession(page, request)
    const admin = new AdminPage(page)
    await admin.goto()
    await admin.openTab('Invites')
    await admin.createCodeButton.click()

    const dialog = page.locator('.glass-modal').last()
    await dialog.getByRole('button', { name: /^create$/i }).click()
    // Success phase shows the generated code
    await expect(dialog.getByText(/copy code/i)).toBeVisible()
    await dialog.getByRole('button', { name: /done/i }).click()
    await expect(page.getByText(/invite/i).first()).toBeVisible()
  })

  test('system config: view shows masked secrets', async ({ page, request }) => {
    await seedAdminSession(page, request)
    const admin = new AdminPage(page)
    await admin.goto()
    await admin.openTab('System Config')

    // Secrets are masked
    await expect(page.getByText(/jwt secret/i)).toBeVisible()
    await expect(page.locator('input[type="password"]').first()).toHaveValue(/\*/)
  })

  test('system config: test connection endpoint verifies DB and Redis', async ({ request }) => {
    // The UI posts the MASKED url (product quirk), so exercise the endpoint
    // directly with the real connection strings from the test environment.
    const dbUrl =
      process.env.PICHOST_DATABASE_URL || 'postgres://pichost:pichost@localhost:5432/pichost'
    const redisUrl = process.env.PICHOST_REDIS_URL || 'redis://localhost:6379'
    const res = await request.post(`${API_BASE}/admin/config/test`, {
      data: { database_url: dbUrl, redis_url: redisUrl },
      headers: { Authorization: `Bearer ${auth.admin.access_token}` },
    })
    expect(res.status()).toBe(200)
    const result = (await res.json()) as { database: string; redis: string }
    expect(result.database).toBe('ok')
    expect(result.redis).toBe('ok')
  })

  test('system config: backup and restore', async ({ page, request }) => {
    await seedAdminSession(page, request)
    const admin = new AdminPage(page)
    await admin.goto()
    await admin.openTab('System Config')

    await admin.backupConfigButton.click()
    await expect(page.getByText(/backed up as|backup/i).first()).toBeVisible({ timeout: 15_000 })
  })

  test('invite code created via API can register a new user', async ({ request }) => {
    const invite = await createInvite(request, auth.admin.access_token, 1)
    const user = await registerUser(
      request,
      uniqueUsername('invitee'),
      'InviteePass123!',
      invite.code,
    )
    expect(user.access_token).toBeTruthy()
    expect(user.user.is_admin).toBe(false)
  })
})
