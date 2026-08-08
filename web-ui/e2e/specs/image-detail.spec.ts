/**
 * Image detail — metadata, inline rename, category assignment, link formats, delete.
 * Self-contained: uploads its own fixture in beforeAll.
 */
import { test, expect } from '@playwright/test'
import { ensureAuth, seedUserSession, type AuthContext } from '../helpers/auth'
import { FIXTURES } from '../helpers/fixtures'
import { ImageDetailPage } from '../page-objects/image-detail.po'
import { API_BASE, uploadFile } from '../helpers/api'
import { selectGlassOption, expectGlassValue } from '../helpers/glass-select'

let auth: AuthContext
let imageId: string
let originalName: string
let imagePublicKey: string

test.beforeAll(async ({ request }) => {
  auth = await ensureAuth(request)
  const result = await uploadFile(request, auth.user.access_token, FIXTURES.png200, 'image/png')
  imageId = result.id
  originalName = result.original_name
  imagePublicKey = result.public_key
})

test.describe.serial('image-detail', () => {
  test('shows metadata (name, status, dimensions)', async ({ page, request }) => {
    await seedUserSession(page, request)
    const detail = new ImageDetailPage(page)
    await detail.goto(imageId)
    await expect(page.getByText('200 × 200px')).toBeVisible()
    await expect(page.getByText('image/png')).toBeVisible()
    await expect(page.getByText(originalName).first()).toBeVisible()
  })

  test('inline rename updates the name', async ({ page, request }) => {
    await seedUserSession(page, request)
    const detail = new ImageDetailPage(page)
    await detail.goto(imageId)
    const newName = `renamed-${Date.now()}.png`
    await detail.rename(newName)
    await expect(page.getByText(newName).first()).toBeVisible()
  })

  test('link format selector switches between URL/Markdown/HTML/BBCode', async ({ page, request }) => {
    await seedUserSession(page, request)
    const detail = new ImageDetailPage(page)
    await detail.goto(imageId)

    // URL
    await selectGlassOption(detail.linkFormatSelect, 'URL')
    const urlValue = await detail.linkValue.innerText()
    expect(urlValue).toContain('/u/')

    // Markdown
    await selectGlassOption(detail.linkFormatSelect, 'Markdown')
    const mdValue = await detail.linkValue.innerText()
    expect(mdValue).toMatch(/^!\[.*\]\(.*\)$/)

    // HTML
    await selectGlassOption(detail.linkFormatSelect, 'HTML')
    const htmlValue = await detail.linkValue.innerText()
    expect(htmlValue).toMatch(/^<img /)

    // BBCode
    await selectGlassOption(detail.linkFormatSelect, 'BBCode')
    const bbValue = await detail.linkValue.innerText()
    expect(bbValue).toMatch(/^\[img\]/)
  })

  test('category assignment from dropdown', async ({ page, request }) => {
    await seedUserSession(page, request)
    // Create a category via API
    const catName = `detail-cat-${Date.now()}`
    const res = await request.post(`${API_BASE}/categories`, {
      data: { name: catName },
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
    })
    expect(res.status()).toBe(201)
    const { id: categoryId } = await res.json()

    const detail = new ImageDetailPage(page)
    await detail.goto(imageId)
    await selectGlassOption(detail.categorySelect, catName)
    // Mutation completes → combobox trigger now shows the category label
    await expectGlassValue(detail.categorySelect, catName)
  })

  test('category can be cleared back to None', async ({ page, request }) => {
    await seedUserSession(page, request)
    const detail = new ImageDetailPage(page)
    await detail.goto(imageId)
    await selectGlassOption(detail.categorySelect, 'None')
    await expectGlassValue(detail.categorySelect, 'None')
  })

  test('delete requires two-step confirmation', async ({ page, request }) => {
    await seedUserSession(page, request)
    // Upload a disposable image for deletion
    const result = await uploadFile(page.request, auth.user.access_token, FIXTURES.png1x1, 'image/png')
    const detail = new ImageDetailPage(page)
    await detail.goto(result.id)

    await detail.deleteButton.click()
    await expect(detail.confirmDeleteButton).toBeVisible()
    // Cancel first
    await page.getByRole('button', { name: /cancel/i }).click()
    await expect(detail.confirmDeleteButton).not.toBeVisible()

    // Confirm second time deletes and returns to gallery/dashboard
    await detail.deleteButton.click()
    await detail.confirmDeleteButton.click()
    await expect(page).toHaveURL(/\/(dashboard|gallery)/)
  })

  test('public serving works via public key', async ({ request }) => {
    expect(imagePublicKey).toBeTruthy()
    const res = await request.get(`/u/${imagePublicKey}`)
    expect(res.status()).toBe(200)
    expect(res.headers()['content-type']).toContain('image/')
  })
})

test.describe('image detail on mobile', () => {
  test.use({ viewport: { width: 375, height: 667 }, hasTouch: true })

  test('rename pencil visible without hover on touch', async ({ page, request }) => {
    await seedUserSession(page, request)
    const detail = new ImageDetailPage(page)
    await detail.goto(imageId)
    const pencil = page.locator('svg.lucide-pencil')
    await expect(pencil).toBeVisible()
    // Playwright ignores opacity for visibility — assert the computed value:
    // below md the pencil must be fully opaque without any hover.
    await expect(pencil).toHaveCSS('opacity', '1')
  })
})
