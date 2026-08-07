import { describe, it, expect } from 'vitest'
import { getErrorCode, isErrorCode } from './errors'

function makeError(code: string | null, status = 400): unknown {
  const err = new Error('HTTP Error') as any
  err.response = { status, json: async () => ({ error: 'msg', code }) }
  return err
}
describe('getErrorCode', () => {
  it('extracts code from HTTPError body', async () => {
    expect(await getErrorCode(makeError('auth.invalid_token'))).toBe('auth.invalid_token')
    expect(await getErrorCode(makeError(null))).toBeNull()
    expect(await getErrorCode(new Error('plain'))).toBeNull()
  })
  it('prefers an attached code over re-parsing response.json()', async () => {
    let parsed = false
    const err = new Error('HTTP Error') as any
    err.code = 'auth.invalid_token'
    err.response = {
      status: 400,
      json: async () => {
        parsed = true
        return { error: 'msg', code: 'other.code' }
      },
    }
    expect(await getErrorCode(err)).toBe('auth.invalid_token')
    expect(parsed).toBe(false)
  })
  it('ignores a non-string attached code and falls back to the body', async () => {
    const err = new Error('HTTP Error') as any
    err.code = 42
    err.response = { status: 400, json: async () => ({ error: 'msg', code: 'other.code' }) }
    expect(await getErrorCode(err)).toBe('other.code')
  })
})
describe('isErrorCode', () => {
  it('matches behavior codes', async () => {
    expect(await isErrorCode(makeError('auth.invalid_token'), 'auth.invalid_token')).toBe(true)
    expect(await isErrorCode(makeError('other'), 'auth.invalid_token')).toBe(false)
  })
})
