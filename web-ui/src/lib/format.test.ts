import { describe, it, expect } from 'vitest'
import { formatBytes, formatDate, formatNumber } from './format'

describe('formatBytes', () => {
  it('formats with units and clamps index', () => {
    expect(formatBytes(0, 'en')).toBe('0 B')
    expect(formatBytes(1536, 'en')).toBe('1.5 KB')
    expect(formatBytes(1024 ** 5, 'en')).toBe('1024 TB') // clamp 不越界
  })
  it('formats numbers locale-aware', () => {
    expect(formatNumber(1234567, 'en')).toContain(',')
  })
})
describe('formatDate', () => {
  it('renders without throwing for both locales', () => {
    expect(formatDate(0, 'en')).toBeTruthy()
    expect(formatDate(0, 'zh-CN')).toBeTruthy()
  })
})
