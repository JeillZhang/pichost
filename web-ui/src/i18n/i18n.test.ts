import { describe, it, expect, beforeEach } from 'vitest'
import i18n, { getCurrentLocale } from './index'

describe('i18n engine', () => {
  beforeEach(async () => {
    localStorage.clear()
    await i18n.changeLanguage('en')
  })
  it('returns normalized locale after changeLanguage', async () => {
    await i18n.changeLanguage('zh-CN')
    expect(getCurrentLocale()).toBe('zh-CN')
    expect(document.documentElement.lang).toBe('zh-CN')
  })
  it('normalizes zh to zh-CN', async () => {
    await i18n.changeLanguage('zh')
    expect(getCurrentLocale()).toBe('zh-CN')
  })
})
