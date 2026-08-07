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
})
describe('isErrorCode', () => {
  it('matches behavior codes', async () => {
    expect(await isErrorCode(makeError('auth.invalid_token'), 'auth.invalid_token')).toBe(true)
    expect(await isErrorCode(makeError('other'), 'auth.invalid_token')).toBe(false)
  })
})
