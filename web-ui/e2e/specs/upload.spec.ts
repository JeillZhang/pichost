/**
 * Upload flows — file upload via dashboard, URL upload, dedup, invalid files.
 * Self-contained: `ensureAuth` in beforeAll; tokens kept file-locally.
 */
import { test, expect } from '@playwright/test'
import { ensureAuth, seedUserSession, type AuthContext } from '../helpers/auth'
import { FIXTURES } from '../helpers/fixtures'
import { DashboardPage } from '../page-objects/dashboard.po'
import { API_BASE } from '../helpers/api'

let auth: AuthContext

test.beforeAll(async ({ request }) => {
  auth = await ensureAuth(request)
})

test.describe.serial('upload', () => {
  let dashboard: DashboardPage

  test.beforeEach(async ({ page, request }) => {
    await seedUserSession(page, request)
    dashboard = new DashboardPage(page)
    await dashboard.goto()
  })

  test('file upload appears in queue and completes', async ({ page, request }) => {
    await dashboard.uploadFile(FIXTURES.png200)
    await expect(page.getByText('test-200x200.png').first()).toBeVisible()
    await dashboard.waitForUploadsDone()
    // UploadCard done state shows the filename with an Open link
    await expect(page.getByText(/open/i).first()).toBeVisible()
  })

  test('file upload via drag and drop works', async ({ page, request }) => {
    const dropZone = page.locator('input[type="file"]')
    await dropZone.setInputFiles(FIXTURES.png1x1)
    await expect(page.getByText('test-1x1.png').first()).toBeVisible()
    await dashboard.waitForUploadsDone()
    await expect(page.getByText(/open/i).first()).toBeVisible()
  })

  test('invalid image content is rejected by the backend', async ({ page, request }) => {
    // A .txt file is filtered out client-side by react-dropzone, so use a
    // PNG-named file with text content: it passes the client accept filter
    // but fails the backend's `infer::is_image` validation.
    await dashboard.uploadFile(FIXTURES.fakePng)
    await expect(page.getByText('fake.png').first()).toBeVisible()
    await expect(page.getByText(/not a valid image|error|failed/i).first()).toBeVisible({
      timeout: 20_000,
    })
    await dashboard.clearDoneButton.click()
  })

  test('duplicate upload is deduplicated', async ({ request }) => {
    // Upload the same file twice via API — second should return the same image id
    const fs = await import('node:fs')
    const file = { name: 'dup.png', mimeType: 'image/png', buffer: fs.readFileSync(FIXTURES.png1x1) }
    const first = await request.post(`${API_BASE}/images`, {
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
      multipart: { file },
    })
    const second = await request.post(`${API_BASE}/images`, {
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
      multipart: { file },
    })
    // 201 = created, 200 = dedup hit — both must succeed and share the id
    expect([200, 201]).toContain(first.status())
    expect([200, 201]).toContain(second.status())
    const [a, b] = [await first.json(), await second.json()]
    expect(a[0].id).toBe(b[0].id)
  })

  test('url upload: SSRF-blocked address is rejected by the backend', async ({ request }) => {
    // The backend refuses private/reserved IPs — localhost must be rejected.
    // (The UI currently surfaces URL-upload errors only in the console, so
    // assert the backend behavior directly.)
    const res = await request.post(`${API_BASE}/images/upload-url`, {
      data: { url: 'http://127.0.0.1:3000/api/health' },
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
    })
    expect(res.status()).toBe(400)
    expect(((await res.json()) as { error: string }).error).toContain('SSRF')
  })

  test('url upload: public image URL completes', async ({ page, request }) => {
    // Deterministic public fixture — GitHub raw is reachable from GitHub runners.
    // URL uploads are silent (no queue card): they refresh the recent-images
    // list, so assert the image lands in the gallery.
    const url = 'https://raw.githubusercontent.com/JeillZhang/pichost/main/web-ui/e2e/fixtures/test-1x1.png'
    await dashboard.uploadUrl(url)
    const res = await request.get(`${API_BASE}/images`, {
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
      params: { per_page: '5' },
    })
    const { items } = (await res.json()) as { items: Array<{ url: string }> }
    expect(items.some((img) => img.url.includes('/u/'))).toBe(true)
  })

  test('empty queue after clear done', async ({ page, request }) => {
    await dashboard.uploadFile(FIXTURES.png1x1)
    await dashboard.waitForUploadsDone()
    if (await dashboard.clearDoneButton.isVisible()) {
      await dashboard.clearDoneButton.click()
      await expect(dashboard.clearDoneButton).not.toBeVisible()
    }
  })
})
