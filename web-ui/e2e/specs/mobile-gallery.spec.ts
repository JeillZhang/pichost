import { test, expect } from '@playwright/test'
import { seedUserSession, ensureAuth } from '../helpers/auth'
import { createCategory } from '../helpers/api'

test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

test.beforeAll(async ({ request }) => {
  // Registers the E2E user in a freshly-reset test DB (same pattern as
  // mobile-nav.spec.ts) — seedUserSession assumes the user exists.
  await ensureAuth(request)
})

test.describe.serial('mobile gallery', () => {
  test('category drawer opens and filters images', async ({ page, request }) => {
    await seedUserSession(page, request)
    const auth = await ensureAuth(request)
    const cat = await createCategory(request, auth.user.access_token, `mobile-cat-${Date.now()}`)
    await page.goto('/gallery')

    // Desktop sidebar hidden on mobile — cat.name may still be in the DOM
    // (display:none aside), so assert it is not visible rather than absent.
    await expect(page.getByText(cat.name)).toBeHidden()

    // Open category drawer
    await page.getByRole('button', { name: /categories|分类/i }).click()
    const drawer = page.getByRole('dialog')
    await expect(drawer.getByText(cat.name)).toBeVisible()

    // Select the category → drawer closes
    await drawer.getByText(cat.name).click()
    await expect(page.getByRole('dialog')).toHaveCount(0)
    await expect(page).toHaveURL(/category_id=/)
  })
})
