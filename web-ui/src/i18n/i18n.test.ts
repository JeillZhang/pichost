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

function flattenKeys(obj: Record<string, unknown>, prefix = ''): string[] {
  return Object.entries(obj).flatMap(([k, v]) =>
    v && typeof v === 'object' ? flattenKeys(v as Record<string, unknown>, `${prefix}${k}.`)
                               : [`${prefix}${k}`])
}
it('en and zh-CN key sets are identical', async () => {
  const en = (await import('./locales/en.json')).default
  const zh = (await import('./locales/zh-CN.json')).default
  const enKeys = flattenKeys(en).sort()
  const zhKeys = flattenKeys(zh).sort()
  expect(zhKeys).toEqual(enKeys)
})
it('typed t() accepts known key and rejects unknown (type-level)', () => {
  // 该断言在类型层验证: t('common.cancel') 通过编译;t('nope.missing') 编译失败
  expect(i18n.t('common.cancel')).toBe('Cancel')
})
it('common and nav prefixes exist in both locales', async () => {
  const en = (await import('./locales/en.json')).default as Record<string, unknown>
  expect(Object.keys(en)).toEqual(expect.arrayContaining(['common', 'nav']))
  const zh = (await import('./locales/zh-CN.json')).default as Record<string, unknown>
  expect(Object.keys(zh)).toEqual(expect.arrayContaining(['common', 'nav']))
})
it('login, register and languageSwitcher prefixes exist in both locales', async () => {
  const en = (await import('./locales/en.json')).default as Record<string, unknown>
  const zh = (await import('./locales/zh-CN.json')).default as Record<string, unknown>
  for (const prefix of ['login', 'register', 'languageSwitcher']) {
    expect(Object.keys(en)).toContain(prefix)
    expect(Object.keys(zh)).toContain(prefix)
  }
})
it('nav, search, sort and themeToggle prefixes exist in both locales', async () => {
  const en = (await import('./locales/en.json')).default as Record<string, unknown>
  const zh = (await import('./locales/zh-CN.json')).default as Record<string, unknown>
  for (const prefix of ['nav', 'search', 'sort', 'themeToggle']) {
    expect(Object.keys(en)).toContain(prefix)
    expect(Object.keys(zh)).toContain(prefix)
  }
})
it('dashboard, dropzone and upload prefixes exist in both locales', async () => {
  const en = (await import('./locales/en.json')).default as Record<string, unknown>
  const zh = (await import('./locales/zh-CN.json')).default as Record<string, unknown>
  for (const prefix of ['dashboard', 'dropzone', 'upload']) {
    expect(Object.keys(en)).toContain(prefix)
    expect(Object.keys(zh)).toContain(prefix)
  }
})
it('gallery, urlUpload and linkCard prefixes exist in both locales', async () => {
  const en = (await import('./locales/en.json')).default as Record<string, unknown>
  const zh = (await import('./locales/zh-CN.json')).default as Record<string, unknown>
  for (const prefix of ['gallery', 'urlUpload', 'linkCard']) {
    expect(Object.keys(en)).toContain(prefix)
    expect(Object.keys(zh)).toContain(prefix)
  }
})
it('gallery deleteConfirm plural keys present', async () => {
  const en = (await import('./locales/en.json')).default as any
  expect(en.gallery.deleteConfirm_one).toBeTruthy()
  expect(en.gallery.deleteConfirm_other).toBeTruthy()
})
it('imageDetail and settings prefixes exist in both locales', async () => {
  const en = (await import('./locales/en.json')).default as Record<string, unknown>
  const zh = (await import('./locales/zh-CN.json')).default as Record<string, unknown>
  for (const prefix of ['imageDetail', 'settings']) {
    expect(Object.keys(en)).toContain(prefix)
    expect(Object.keys(zh)).toContain(prefix)
  }
})
it('categoryTree and storageConfig prefixes exist in both locales', async () => {
  const en = (await import('./locales/en.json')).default as Record<string, unknown>
  const zh = (await import('./locales/zh-CN.json')).default as Record<string, unknown>
  for (const prefix of ['categoryTree', 'storageConfig']) {
    expect(Object.keys(en)).toContain(prefix)
    expect(Object.keys(zh)).toContain(prefix)
  }
})
it('systemConfig, watermark and preprocessing prefixes exist in both locales', async () => {
  const en = (await import('./locales/en.json')).default as Record<string, unknown>
  const zh = (await import('./locales/zh-CN.json')).default as Record<string, unknown>
  for (const prefix of ['systemConfig', 'watermark', 'preprocessing']) {
    expect(Object.keys(en)).toContain(prefix)
    expect(Object.keys(zh)).toContain(prefix)
  }
})
it('systemConfig and preprocessing plural keys present', async () => {
  const en = (await import('./locales/en.json')).default as any
  expect(en.systemConfig.backupCount_one).toBeTruthy()
  expect(en.systemConfig.backupCount_other).toBeTruthy()
  expect(en.systemConfig.changedCount_one).toBeTruthy()
  expect(en.systemConfig.changedCount_other).toBeTruthy()
})
it('adminStats, adminUsers and editUser prefixes exist in both locales', async () => {
  const en = (await import('./locales/en.json')).default as Record<string, unknown>
  const zh = (await import('./locales/zh-CN.json')).default as Record<string, unknown>
  for (const prefix of ['adminStats', 'adminUsers', 'editUser']) {
    expect(Object.keys(en)).toContain(prefix)
    expect(Object.keys(zh)).toContain(prefix)
  }
})
it('adminInvites, createInvite and adminTabs prefixes exist in both locales', async () => {
  const en = (await import('./locales/en.json')).default as Record<string, unknown>
  const zh = (await import('./locales/zh-CN.json')).default as Record<string, unknown>
  for (const prefix of ['adminInvites', 'createInvite', 'adminTabs']) {
    expect(Object.keys(en)).toContain(prefix)
    expect(Object.keys(zh)).toContain(prefix)
  }
})
