import { describe, it, expect } from 'vitest'
import { clampLeft } from './GlassSelect'

describe('clampLeft', () => {
  it('returns left unchanged when it fits', () => {
    expect(clampLeft(100, 200, 375)).toBe(100)
  })
  it('clamps when right edge exceeds viewport', () => {
    expect(clampLeft(300, 200, 375)).toBe(167) // 300+200=500 > 375-8 → maxLeft=167
  })
  it('never goes below 8px margin', () => {
    expect(clampLeft(-50, 200, 375)).toBe(8)
  })
})
