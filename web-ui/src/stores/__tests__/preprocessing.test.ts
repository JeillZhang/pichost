import { describe, it, expect, beforeEach } from 'vitest'
import { DEFAULT_PREFS } from '../../types/preprocessing'
import { usePreprocessingStore } from '../preprocessing'

describe('DEFAULT_PREFS', () => {
  it('has all operations disabled by default', () => {
    expect(DEFAULT_PREFS.stripExif).toBe(false)
    expect(DEFAULT_PREFS.resize.enabled).toBe(false)
    expect(DEFAULT_PREFS.formatConvert.enabled).toBe(false)
    expect(DEFAULT_PREFS.compression.enabled).toBe(false)
    expect(DEFAULT_PREFS.rotate.enabled).toBe(false)
  })

  it('has sensible default values for enabled state', () => {
    expect(DEFAULT_PREFS.resize.maxWidth).toBe(1920)
    expect(DEFAULT_PREFS.resize.maxHeight).toBe(1920)
    expect(DEFAULT_PREFS.formatConvert.targetFormat).toBe('image/webp')
    expect(DEFAULT_PREFS.formatConvert.quality).toBe(85)
    expect(DEFAULT_PREFS.compression.quality).toBe(80)
    expect(DEFAULT_PREFS.rotate.degrees).toBe(0)
  })
})

describe('usePreprocessingStore', () => {
  beforeEach(() => {
    localStorage.clear()
    // Reset store to defaults
    const state = usePreprocessingStore.getState()
    state.resetAll()
  })

  it('initializes with DEFAULT_PREFS when localStorage is empty', () => {
    const state = usePreprocessingStore.getState()
    expect(state.stripExif).toBe(false)
    expect(state.resize.enabled).toBe(false)
  })

  it('persists to localStorage on state change', () => {
    usePreprocessingStore.getState().setStripExif(true)
    const raw = localStorage.getItem('pichost-preprocessing')
    expect(raw).not.toBeNull()
    const parsed = JSON.parse(raw!)
    expect(parsed.state.stripExif).toBe(true)
  })

  it('updateResize updates resize settings', () => {
    usePreprocessingStore.getState().updateResize({ enabled: true, maxWidth: 1024, maxHeight: 768 })
    const state = usePreprocessingStore.getState()
    expect(state.resize.enabled).toBe(true)
    expect(state.resize.maxWidth).toBe(1024)
    expect(state.resize.maxHeight).toBe(768)
  })

  it('hasAnyEnabled returns false when all disabled', () => {
    expect(usePreprocessingStore.getState().hasAnyEnabled()).toBe(false)
  })

  it('hasAnyEnabled returns true when stripExif is on', () => {
    usePreprocessingStore.getState().setStripExif(true)
    expect(usePreprocessingStore.getState().hasAnyEnabled()).toBe(true)
  })

  it('resetAll restores defaults', () => {
    usePreprocessingStore.getState().setStripExif(true)
    usePreprocessingStore.getState().resetAll()
    expect(usePreprocessingStore.getState().stripExif).toBe(false)
  })
})
