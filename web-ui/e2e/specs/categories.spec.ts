/**
 * Categories — CRUD via API and gallery sidebar tree.
 */
import { test, expect } from '@playwright/test'
import { ensureAuth, seedUserSession, type AuthContext } from '../helpers/auth'
import { API_BASE, createCategory } from '../helpers/api'

let auth: AuthContext
let imageId: string

test.beforeAll(async ({ request }) => {
  auth = await ensureAuth(request)
  // Seed one image for the delete-with-images test
  const list = await request.get(`${API_BASE}/images?per_page=1`, {
    headers: { Authorization: `Bearer ${auth.user.access_token}` },
  })
  const { items } = await list.json()
  if (items.length > 0) {
    imageId = items[0].id
  }
})

test.describe.serial('categories', () => {
  test('create root and nested category via API', async ({ request }) => {
    const root = await createCategory(request, auth.user.access_token, `root-${Date.now()}`)
    const child = await createCategory(request, auth.user.access_token, `child-${Date.now()}`, root.id)
    expect(root.id).toBeTruthy()
    expect(child.id).toBeTruthy()

    // Tree lists the root with its child
    const tree = await request.get(`${API_BASE}/categories`, {
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
    })
    const roots = (await tree.json()) as Array<{ id: string; children: unknown[] }>
    const found = roots.find((c) => c.id === root.id)
    expect(found).toBeTruthy()
    expect(found!.children).toHaveLength(1)
  })

  test('depth limit: child of a child is rejected', async ({ request }) => {
    const root = await createCategory(request, auth.user.access_token, `depth-root-${Date.now()}`)
    const child = await createCategory(request, auth.user.access_token, `depth-child-${Date.now()}`, root.id)
    const res = await request.post(`${API_BASE}/categories`, {
      data: { name: 'too-deep', parent_id: child.id },
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
    })
    expect(res.status()).toBe(400)
  })

  test('duplicate name under same parent is rejected', async ({ request }) => {
    // Two root categories may share a name (parent_id NULL is distinct in the
    // unique index) — the conflict only fires under a non-NULL parent.
    const parent = await createCategory(request, auth.user.access_token, `dup-parent-${Date.now()}`)
    const name = `dup-child-${Date.now()}`
    await createCategory(request, auth.user.access_token, name, parent.id)
    const res = await request.post(`${API_BASE}/categories`, {
      data: { name, parent_id: parent.id },
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
    })
    expect(res.status()).toBe(409)
  })

  test('rename category via PATCH', async ({ request }) => {
    const cat = await createCategory(request, auth.user.access_token, `rename-me-${Date.now()}`)
    const res = await request.patch(`${API_BASE}/categories/${cat.id}`, {
      data: { name: 'renamed-category' },
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
    })
    expect(res.status()).toBe(200)
    expect((await res.json()).name).toBe('renamed-category')
  })

  test('delete category with images keeps images (category_id → null)', async ({ request }) => {
    test.skip(!imageId, 'no seed image available')
    const cat = await createCategory(request, auth.user.access_token, `delete-with-images-${Date.now()}`)
    // Move the shared image into this category
    const move = await request.post(`${API_BASE}/images/${imageId}/move`, {
      data: { category_id: cat.id },
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
    })
    expect(move.status()).toBe(200)

    const del = await request.delete(`${API_BASE}/categories/${cat.id}`, {
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
    })
    expect(del.status()).toBe(200)

    // Image still exists, now uncategorized
    const img = await request.get(`${API_BASE}/images/${imageId}`, {
      headers: { Authorization: `Bearer ${auth.user.access_token}` },
    })
    expect(img.status()).toBe(200)
    expect((await img.json()).category_id).toBeNull()
  })

  test('category tree renders in gallery sidebar', async ({ page, request }) => {
    await seedUserSession(page, request)
    const cat = await createCategory(request, auth.user.access_token, `sidebar-${Date.now()}`)
    await page.goto('/gallery')
    await expect(page.getByText(cat.name)).toBeVisible()
  })
})
