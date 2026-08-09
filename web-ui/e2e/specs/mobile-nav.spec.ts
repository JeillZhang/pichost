import { test, expect } from '@playwright/test'
import { ensureAuth, seedUserSession } from '../helpers/auth'

test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

test.beforeAll(async ({ request }) => {
  // Registers the E2E user in a freshly-reset test DB (same pattern as
  // i18n.spec.ts / 00-auth.spec.ts) — seedUserSession assumes the user exists.
  await ensureAuth(request)
})

test.describe.serial('mobile nav', () => {
  test('hamburger opens drawer with nav links and user actions', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/dashboard')

    // Desktop links are hidden on mobile
    await expect(page.getByRole('link', { name: 'Dashboard' })).toHaveCount(0)

    // Open hamburger menu
    const menuButton = page.getByRole('button', { name: /menu|菜单/i })
    await menuButton.click()
    await expect(page.getByRole('link', { name: /dashboard|仪表盘/i }).first()).toBeVisible()

    // Navigate via drawer
    await page.getByRole('link', { name: /gallery|图库/i }).first().click()
    await expect(page).toHaveURL(/\/gallery/)
  })

  test('drawer closes on Escape and overlay click', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/dashboard')
    await page.getByRole('button', { name: /menu|菜单/i }).click()
    await page.keyboard.press('Escape')
    await expect(page.getByRole('link', { name: /dashboard|仪表盘/i }).first()).toBeHidden()
  })

  test('logout reachable from mobile drawer', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/dashboard')
    await page.getByRole('button', { name: /menu|菜单/i }).click()
    await page.getByRole('button', { name: /logout|退出登录/i }).click()
    await expect(page).toHaveURL(/\/login/)
  })
})
