/**
 * Settings — profile, password, storage usage, storage backends, watermark, preprocessing.
 */
import { test, expect } from '@playwright/test'
import { ensureAuth, seedUserSession, type AuthContext } from '../helpers/auth'
import { TEST_USER, uniqueUsername } from '../helpers/fixtures'
import { SettingsPage } from '../page-objects/settings.po'
import { API_BASE } from '../helpers/api'

let auth: AuthContext

test.beforeAll(async ({ request }) => {
  auth = await ensureAuth(request)
})

test.describe.serial('settings', () => {
  test.beforeEach(async ({ page, request }) => {
    await seedUserSession(page, request)
  })

  test('profile section shows current values and saves', async ({ page, request }) => {
    const settings = new SettingsPage(page)
    await settings.goto('profile')
    await expect(settings.usernameInput).toHaveValue(TEST_USER.username)

    const newName = uniqueUsername('newname')
    await settings.usernameInput.fill(newName)
    await settings.saveProfileButton.click()
    await expect(page.getByText(/profile updated/i)).toBeVisible()

    // Revert so other tests keep working
    await settings.usernameInput.fill(TEST_USER.username)
    await settings.saveProfileButton.click()
    await expect(page.getByText(/profile updated/i)).toBeVisible()
  })

  test('password change with wrong current password fails', async ({ page, request }) => {
    // Backend rejects the wrong current password
    const res = await request.post(`${API_BASE}/users/me/password`, {
      data: { current_password: 'definitely-wrong', new_password: 'NewPass123!' },
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
    })
    expect(res.status()).toBe(401)
    expect(((await res.json()) as { error: string }).error).toContain(
      'current password incorrect',
    )

    // UI shows the backend-localized error toast after submit
    const settings = new SettingsPage(page)
    await settings.goto('password')
    await settings.changePassword('definitely-wrong', 'NewPass123!')
    await expect(page.getByText(/current password incorrect/i).first()).toBeVisible({
      timeout: 15_000,
    })
  })

  test('password change with short new password rejected client-side', async ({ page, request }) => {
    // The new-password input has minLength=8, so the browser's native
    // validation blocks submission before React's handler runs.
    const settings = new SettingsPage(page)
    await settings.goto('password')
    await settings.changePassword(TEST_USER.password, 'short')
    await page.waitForTimeout(800)
    // No success toast, and the old password still works
    await expect(page.getByText(/password changed/i)).not.toBeVisible()
    const check = await request.post(`${API_BASE}/auth/login`, {
      data: { username: TEST_USER.username, password: TEST_USER.password },
    })
    expect(check.status()).toBe(200)
  })

  test('storage usage section renders quota bar', async ({ page, request }) => {
    const settings = new SettingsPage(page)
    await settings.goto('storage-usage')
    await expect(page.getByText(/B|KB|MB|GB/).first()).toBeVisible()
  })

  test('storage backends section shows empty state and add modal opens', async ({ page, request }) => {
    const settings = new SettingsPage(page)
    await settings.goto('storage-configs')
    await expect(page.getByRole('heading', { name: /storage configs/i })).toBeVisible()
    const addButton = page.getByRole('button', { name: /^add$/i })
    await addButton.click()
    await expect(page.getByText('Add Storage Config')).toBeVisible()
    // Cancel closes the portal modal
    await page.getByRole('button', { name: /^cancel$/i }).click()
    await expect(page.getByText('Add Storage Config')).not.toBeVisible()
  })

  test('creating a storage config without a token is rejected', async ({ page, request }) => {
    const settings = new SettingsPage(page)
    await settings.goto('storage-configs')
    await page.getByRole('button', { name: /^add$/i }).click()
    // ConfigModal inputs have no htmlFor — anchor on label text
    await page
      .locator('label', { hasText: /^name$/i })
      .locator('xpath=following-sibling::input')
      .fill('bad-config')
    await page
      .locator('label', { hasText: /repo/i })
      .locator('xpath=following-sibling::input')
      .fill('owner/repo')
    // The Token input is required — native HTML validation blocks submit,
    // so no config is created and the modal stays open.
    await page.getByRole('button', { name: /^create$/i }).click()
    await page.waitForTimeout(800)
    await expect(page.getByText('Add Storage Config')).toBeVisible()
    const list = await request.get(`${API_BASE}/users/me/storage-configs`, {
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
    })
    expect((await list.json()) as unknown[]).toHaveLength(0)
    await page.getByRole('button', { name: /^cancel$/i }).click()
    await expect(page.getByText('Add Storage Config')).not.toBeVisible()
  })

  test('watermark section toggles and saves', async ({ page, request }) => {
    const settings = new SettingsPage(page)
    await settings.goto('watermark')

    const enableCheckbox = page.getByRole('checkbox').first()
    await enableCheckbox.check()
    // Watermark inputs have no htmlFor — anchor on the label text
    await page
      .locator('label', { hasText: /watermark text/i })
      .locator('xpath=following-sibling::input')
      .fill('E2E Watermark')
    await page.getByRole('button', { name: /^save$/i }).click()
    await expect(page.getByText(/watermark settings saved/i)).toBeVisible()

    // Backend persisted the config
    const me = await request.get(`${API_BASE}/users/me`, {
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
    })
    const profile = await me.json()
    expect(profile.watermark_config?.enabled).toBe(true)
    expect(profile.watermark_config?.text).toBe('E2E Watermark')

    // Clear it so later uploads are not watermarked
    await settings.goto('watermark')
    await page.getByRole('button', { name: /^clear$/i }).click()
    await expect(page.getByText(/watermark settings cleared/i)).toBeVisible()
  })

  test('preprocessing section toggles EXIF stripping', async ({ page, request }) => {
    const settings = new SettingsPage(page)
    await settings.goto('preprocessing')
    await page.getByText(/remove exif/i).click()
    await expect(page.getByText(/exif/i).first()).toBeVisible()
    // Reset so dashboard preprocessing stays off for other tests
    await page.getByRole('button', { name: /reset to defaults/i }).click()
  })

  test('oauth section shows provider links', async ({ page, request }) => {
    const settings = new SettingsPage(page)
    await settings.goto('oauth')
    await expect(page.getByRole('link', { name: /link github/i })).toBeVisible()
    await expect(page.getByRole('link', { name: /link google/i })).toBeVisible()
  })
})

test.describe('storage config dialogs on mobile', () => {
  test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

  test('config modal is bottom sheet on mobile', async ({ page, request }) => {
    await seedUserSession(page, request)
    await page.goto('/settings')
    await page
      .getByRole('button', { name: /storage backends|存储后端/i })
      .click()
    await page.getByRole('button', { name: /add|添加/i }).click()
    const panel = page.locator('.glass-modal')
    await expect(panel).toBeVisible()
    const box = await panel.boundingBox()
    const vh = page.viewportSize()!.height
    expect(box!.y + box!.height).toBeGreaterThan(vh - 100)
    await page.keyboard.press('Escape')
    await expect(panel).toBeHidden()
  })
})
