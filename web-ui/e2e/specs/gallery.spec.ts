/**
 * Gallery — listing, search, sort, selection, batch delete.
 * Self-contained: seeds its own images via API in beforeAll.
 */
import { test, expect } from '@playwright/test'
import { ensureAuth, seedUserSession, type AuthContext } from '../helpers/auth'
import { FIXTURES } from '../helpers/fixtures'
import { GalleryPage } from '../page-objects/gallery.po'
import { API_BASE, uploadFile } from '../helpers/api'

let auth: AuthContext

test.beforeAll(async ({ request }) => {
  auth = await ensureAuth(request)
  // Ensure a known set of images exists for list/search tests
  await uploadFile(request, auth.user.access_token, FIXTURES.png1x1, 'image/png')
  await uploadFile(request, auth.user.access_token, FIXTURES.png200, 'image/png')
})

test.describe.serial('gallery', () => {
  test('lists uploaded images', async ({ page, request }) => {
    await seedUserSession(page, request)
    const gallery = new GalleryPage(page)
    await gallery.goto()
    await expect(gallery.imageTiles.first()).toBeVisible()
    await expect(page.getByRole('heading', { name: /gallery/i })).toBeVisible()
  })

  test('search filters by filename', async ({ page, request }) => {
    await seedUserSession(page, request)
    const gallery = new GalleryPage(page)
    await gallery.goto()
    await gallery.searchInput.fill('test-200x200')
    await expect(gallery.imageTiles.first()).toBeVisible()
    await expect(gallery.imageTiles).toHaveCount(1)
  })

  test('search with no results shows empty state', async ({ page, request }) => {
    await seedUserSession(page, request)
    const gallery = new GalleryPage(page)
    await gallery.goto()
    await gallery.searchInput.fill('zzz-nonexistent-name')
    await expect(gallery.emptyState).toBeVisible()
  })

  test('search clear button resets results', async ({ page, request }) => {
    await seedUserSession(page, request)
    const gallery = new GalleryPage(page)
    await gallery.goto()
    await gallery.searchInput.fill('test-200x200')
    await expect(gallery.imageTiles).toHaveCount(1)
    await gallery.searchInput.locator('xpath=following-sibling::button').click()
    await expect(gallery.imageTiles.first()).toBeVisible()
  })

  test('select mode: select and deselect all', async ({ page, request }) => {
    await seedUserSession(page, request)
    const gallery = new GalleryPage(page)
    await gallery.goto()
    // Click the tile's selection checkbox to enter select mode
    await gallery.selectImage(0)
    await expect(gallery.selectAllButton).toBeVisible()
    await gallery.selectAllButton.click()
    await expect(gallery.deselectAllButton).toBeVisible()
    await gallery.deselectAllButton.click()
    // Deselecting everything closes the selection toolbar entirely
    await expect(gallery.selectAllButton).not.toBeVisible()
  })

  test('batch delete removes images', async ({ page, request }) => {
    await seedUserSession(page, request)
    const gallery = new GalleryPage(page)
    await gallery.goto()

    // Upload a disposable image, then delete it via the UI
    const result = await uploadFile(request, auth.user.access_token, FIXTURES.png1x1, 'image/png')
    const before = await request.get(`${API_BASE}/images`, {
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
      params: { search: result.original_name },
    })
    expect(((await before.json()) as { total: number }).total).toBeGreaterThan(0)

    await gallery.searchInput.fill(result.original_name)
    await expect(gallery.imageTiles).toHaveCount(1)
    await gallery.selectImage(0)
    await gallery.deleteButton.click()
    await gallery.confirmDeleteButton.click()
    await expect(gallery.emptyState).toBeVisible()

    const after = await request.get(`${API_BASE}/images`, {
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
      params: { search: result.original_name },
    })
    expect(((await after.json()) as { total: number }).total).toBe(0)
  })

  test('navigates to image detail on tile click', async ({ page, request }) => {
    await seedUserSession(page, request)
    const gallery = new GalleryPage(page)
    await gallery.goto()
    await gallery.imageTiles.first().click()
    await expect(page).toHaveURL(/\/images\//)
  })
})
