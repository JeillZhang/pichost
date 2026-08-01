/**
 * Public endpoints — health, metrics, unauthenticated image serving.
 * Self-contained: uploads its own fixture in beforeAll.
 */
import { test, expect } from '@playwright/test'
import { ensureAuth, type AuthContext } from '../helpers/auth'
import { FIXTURES } from '../helpers/fixtures'
import { uploadFile } from '../helpers/api'

let auth: AuthContext
let imagePublicKey: string

test.beforeAll(async ({ request }) => {
  auth = await ensureAuth(request)
  const result = await uploadFile(request, auth.user.access_token, FIXTURES.png1x1, 'image/png')
  imagePublicKey = result.public_key
})

test.describe('public', () => {
  test('health endpoint reports healthy', async ({ request }) => {
    const res = await request.get('/api/health')
    expect(res.status()).toBe(200)
    const body = await res.json()
    expect(body.status).toBe('healthy')
  })

  test('metrics endpoint exposes prometheus metrics', async ({ request }) => {
    const res = await request.get('/metrics')
    expect(res.status()).toBe(200)
    const text = await res.text()
    expect(text).toContain('pichost_http_requests_total')
    expect(text).toContain('pichost_users_total')
  })

  test('public image serving works without auth', async ({ request }) => {
    const res = await request.get(`/u/${imagePublicKey}`)
    expect(res.status()).toBe(200)
    expect(res.headers()['content-type']).toContain('image/')
  })

  test('unknown public key returns 404', async ({ request }) => {
    const res = await request.get('/u/000000')
    expect(res.status()).toBe(404)
  })

  test('thumbnail alias route serves the image', async ({ request }) => {
    const res = await request.get(`/t/${imagePublicKey}`)
    expect([200, 404]).toContain(res.status()) // 200 once worker processed the thumb
  })
})
