/**
 * Admin tables on mobile — cards replace tables, ConfirmDialog replaces native confirm.
 * Must run on admin session (non-admin is redirected away from /admin).
 */
import { test, expect } from '@playwright/test'
import { ensureAuth, seedAdminSession, type AuthContext } from '../helpers/auth'
import { createInvite } from '../helpers/api'

let auth: AuthContext

test.beforeAll(async ({ request }) => {
  // Registers e2e-admin + e2e-user against the freshly-reset test DB so
  // seedAdminSession can log them in, then creates a fresh unused invite so
  // the invites tab renders at least one card.
  auth = await ensureAuth(request)
  await createInvite(request, auth.admin.access_token, 7)
})

test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

test.describe.serial('mobile admin', () => {
  test('users render as cards on mobile', async ({ page, request }) => {
    await seedAdminSession(page, request)
    await page.goto('/admin')
    await page.getByRole('button', { name: /users|用户/i }).click()
    // Table hidden (kept in DOM by `hidden sm:block` wrapper), cards visible
    await expect(page.locator('table')).toBeHidden()
    await expect(page.locator('[data-testid="user-card"]').first()).toBeVisible()
  })

  test('delete user opens ConfirmDialog on mobile', async ({ page, request }) => {
    await seedAdminSession(page, request)
    await page.goto('/admin')
    await page.getByRole('button', { name: /users|用户/i }).click()
    const card = page.locator('[data-testid="user-card"]').first()
    await card.getByRole('button', { name: /delete|删除/i }).click()
    await expect(page.locator('.glass-modal')).toBeVisible()
  })

  test('invites render as cards on mobile', async ({ page, request }) => {
    await seedAdminSession(page, request)
    await page.goto('/admin')
    await page.getByRole('button', { name: /invites|邀请/i }).click()
    await expect(page.locator('table')).toBeHidden()
    await expect(page.locator('[data-testid="invite-card"]').first()).toBeVisible()
  })
})
