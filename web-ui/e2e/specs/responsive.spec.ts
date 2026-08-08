/**
 * Global mobile responsiveness guard — no horizontal overflow on key pages
 * at a 375px viewport. Admin needs an admin session (non-admin is redirected
 * away from /admin); the rest use the standard E2E user.
 */
import { test, expect } from '@playwright/test'
import { ensureAuth, seedUserSession, seedAdminSession } from '../helpers/auth'

const PAGES = ['/dashboard', '/gallery', '/settings', '/admin']

test.beforeAll(async ({ request }) => {
  // Register the standard E2E admin + user so the seed helpers can log them in.
  await ensureAuth(request)
})

test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

test.describe.serial('no horizontal overflow on mobile', () => {
  for (const path of PAGES) {
    test(`${path} has no horizontal scroll`, async ({ page, request }) => {
      if (path === '/admin') {
        await seedAdminSession(page, request)
      } else {
        await seedUserSession(page, request)
      }
      await page.goto(path)
      await page.waitForTimeout(300)
      const overflow = await page.evaluate(
        () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
      )
      expect(overflow).toBeLessThanOrEqual(1)
    })
  }
})
