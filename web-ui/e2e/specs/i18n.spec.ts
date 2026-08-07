/**
 * i18n — language switching (new UI options introduced by the i18n feature).
 *
 * Covers: NavBar LanguageSwitcher persistence, unauthenticated login-page
 * switcher, and the admin SystemConfig deployment-language field (which
 * hot-reloads the backend so API error messages follow the selected language).
 */
import { test, expect } from '@playwright/test'
import { ensureAuth, seedAdminSession, loginViaUI, type AuthContext } from '../helpers/auth'
import { TEST_USER } from '../helpers/fixtures'
import { API_BASE } from '../helpers/api'
import { selectGlassOption } from '../helpers/glass-select'
import { AdminPage } from '../page-objects/admin.po'

let auth: AuthContext

test.beforeAll(async ({ request }) => {
  auth = await ensureAuth(request)
})

const GLOBE = '.lucide-globe'

test.describe('i18n', () => {
  test('navbar language switcher switches UI language and persists', async ({ page }) => {
    await loginViaUI(page, TEST_USER.username, TEST_USER.password)
    await expect(page).toHaveURL(/\/dashboard/)
    await expect(page.getByText('Gallery').first()).toBeVisible()

    await page.locator(GLOBE).first().click()
    await page.getByText('简体中文').click()
    await expect(page.getByText('图库').first()).toBeVisible()
    expect(await page.evaluate(() => localStorage.getItem('pichost-locale'))).toBe('zh-CN')
    expect(await page.evaluate(() => document.documentElement.lang)).toBe('zh-CN')

    await page.locator(GLOBE).first().click()
    await page.getByText('English').click()
    await expect(page.getByText('Gallery').first()).toBeVisible()
    expect(await page.evaluate(() => localStorage.getItem('pichost-locale'))).toBe('en')
    expect(await page.evaluate(() => document.documentElement.lang)).toBe('en')
  })

  test('login page language switcher works unauthenticated', async ({ page }) => {
    await page.goto('/login')
    await expect(page.getByText('Self-hosted image hosting')).toBeVisible()

    await page.locator(GLOBE).first().click()
    await page.getByText('简体中文').click()
    await expect(page.getByText('自托管图片托管')).toBeVisible()
  })

  test('system config language field applies deployment language to API errors', async ({
    page,
    request,
  }) => {
    await seedAdminSession(page, request)
    const admin = new AdminPage(page)
    await admin.goto()
    await admin.openTab('System Config')

    const combo = page.getByRole('combobox', { name: 'Language' })
    await expect(combo).toBeVisible()
    await selectGlassOption(combo, '简体中文')
    await page.getByRole('button', { name: /^save/i }).click()
    await expect(page.getByText(/config saved/i)).toBeVisible()

    // Backend hot-reloaded: API errors now in Chinese (no Accept-Language header)
    const res = await request.post(`${API_BASE}/auth/login`, {
      data: { username: 'nobody-here', password: 'Whatever123!' },
    })
    expect(res.status()).toBe(401)
    const body = await res.json()
    expect(body.error).toContain('用户名或密码错误')
    expect(body.code).toBe('auth.invalid_credentials')

    // Restore English so later specs are unaffected
    await page.getByRole('combobox', { name: 'Language' }).click()
    await page.getByRole('option', { name: /^English$/ }).click()
    await page.getByRole('button', { name: /^save/i }).click()
    await expect(page.getByText(/config saved/i)).toBeVisible()
  })
})
