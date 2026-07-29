import { create } from 'zustand'
import { DEFAULT_PREFS, STORAGE_KEY, type PreprocessingPrefs } from '../types/preprocessing'

interface PreprocessingStore extends PreprocessingPrefs {
  setStripExif: (v: boolean) => void
  updateResize: (v: PreprocessingPrefs['resize']) => void
  updateFormatConvert: (v: PreprocessingPrefs['formatConvert']) => void
  updateCompression: (v: PreprocessingPrefs['compression']) => void
  updateRotate: (v: PreprocessingPrefs['rotate']) => void
  resetAll: () => void
  hasAnyEnabled: () => boolean
}

function readFromStorage(): PreprocessingPrefs {
  try {
    const raw = localStorage.getItem(STORAGE_KEY)
    if (raw) {
      const parsed = JSON.parse(raw)
      return { ...DEFAULT_PREFS, ...parsed.state }
    }
  } catch {
    /* corrupted data, use defaults */
  }
  return { ...DEFAULT_PREFS }
}

function writeToStorage(state: PreprocessingPrefs): void {
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ state }))
  } catch {
    /* storage full or unavailable */
  }
}

export const usePreprocessingStore = create<PreprocessingStore>((set, get) => ({
  ...readFromStorage(),

  setStripExif: (v) =>
    set((s) => {
      const next = { ...s, stripExif: v }
      writeToStorage(next)
      return { stripExif: v }
    }),

  updateResize: (v) =>
    set((s) => {
      const next = { ...s, resize: v }
      writeToStorage(next)
      return { resize: v }
    }),

  updateFormatConvert: (v) =>
    set((s) => {
      const next = { ...s, formatConvert: v }
      writeToStorage(next)
      return { formatConvert: v }
    }),

  updateCompression: (v) =>
    set((s) => {
      const next = { ...s, compression: v }
      writeToStorage(next)
      return { compression: v }
    }),

  updateRotate: (v) =>
    set((s) => {
      const next = { ...s, rotate: v }
      writeToStorage(next)
      return { rotate: v }
    }),

  resetAll: () =>
    set(() => {
      writeToStorage(DEFAULT_PREFS)
      return { ...DEFAULT_PREFS }
    }),

  hasAnyEnabled: () => {
    const s = get()
    return (
      s.stripExif ||
      s.resize.enabled ||
      s.formatConvert.enabled ||
      s.compression.enabled ||
      s.rotate.enabled
    )
  },
}))
