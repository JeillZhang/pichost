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
      // Measure BOTH doc and body: `overflow-x: clip` on html masks doc-level
      // overflow, but body-level overflow still shows as horizontal scroll.
      const overflow = await page.evaluate(() => {
        const doc = document.documentElement.scrollWidth - document.documentElement.clientWidth
        const body = document.body.scrollWidth - document.body.clientWidth
        return Math.max(doc, body)
      })
      expect(overflow).toBeLessThanOrEqual(1)
    })
  }
})

test('/settings expanded sections fit without overflow', async ({ page, request }) => {
  await seedUserSession(page, request)
  await page.goto('/settings')
  await page.waitForTimeout(300)

  const overflowOf = () =>
    page.evaluate(() => {
      const doc = document.documentElement
      const body = document.body
      return Math.max(
        doc.scrollWidth - doc.clientWidth,
        body.scrollWidth - body.clientWidth,
      )
    })

  await page.getByRole('button', { name: /watermark/i }).click()
  await page.waitForTimeout(200)
  expect(await overflowOf()).toBeLessThanOrEqual(1)

  await page.getByRole('button', { name: /preprocessing/i }).click()
  await page.waitForTimeout(200)
  const toggles = page.locator('input.toggle')
  const toggleCount = await toggles.count()
  for (let i = 0; i < toggleCount; i++) {
    const toggle = toggles.nth(i)
    if (!(await toggle.isChecked())) await toggle.check()
  }
  await page.waitForTimeout(200)
  expect(await overflowOf()).toBeLessThanOrEqual(1)
})
