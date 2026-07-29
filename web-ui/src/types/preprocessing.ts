export interface PreprocessingPrefs {
  stripExif: boolean
  resize: {
    enabled: boolean
    maxWidth: number
    maxHeight: number
  }
  formatConvert: {
    enabled: boolean
    targetFormat: 'image/jpeg' | 'image/png' | 'image/webp'
    quality: number
  }
  compression: {
    enabled: boolean
    quality: number
  }
  rotate: {
    enabled: boolean
    degrees: 0 | 90 | 180 | 270
  }
}

export const DEFAULT_PREFS: PreprocessingPrefs = {
  stripExif: false,
  resize: { enabled: false, maxWidth: 1920, maxHeight: 1920 },
  formatConvert: { enabled: false, targetFormat: 'image/webp', quality: 85 },
  compression: { enabled: false, quality: 80 },
  rotate: { enabled: false, degrees: 0 },
}

export const STORAGE_KEY = 'pichost-preprocessing'
