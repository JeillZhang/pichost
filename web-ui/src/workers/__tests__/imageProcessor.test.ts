import { describe, it, expect } from 'vitest'
import { stripExifFromJpeg, needsProcessing, processFile } from '../imageProcessor'
import { DEFAULT_PREFS } from '../../types/preprocessing'

describe('needsProcessing', () => {
  it('returns false when all prefs are disabled', () => {
    expect(needsProcessing(DEFAULT_PREFS)).toBe(false)
  })

  it('returns true when stripExif is enabled', () => {
    const prefs = { ...DEFAULT_PREFS, stripExif: true }
    expect(needsProcessing(prefs)).toBe(true)
  })

  it('returns true when resize is enabled', () => {
    const prefs = { ...DEFAULT_PREFS, resize: { enabled: true, maxWidth: 100, maxHeight: 100 } }
    expect(needsProcessing(prefs)).toBe(true)
  })

  it('returns true when formatConvert is enabled', () => {
    const prefs = { ...DEFAULT_PREFS, formatConvert: { enabled: true, targetFormat: 'image/webp' as const, quality: 85 } }
    expect(needsProcessing(prefs)).toBe(true)
  })

  it('returns true when compression is enabled', () => {
    const prefs = { ...DEFAULT_PREFS, compression: { enabled: true, quality: 50 } }
    expect(needsProcessing(prefs)).toBe(true)
  })

  it('returns true when rotate is enabled', () => {
    const prefs = { ...DEFAULT_PREFS, rotate: { enabled: true, degrees: 90 as const } }
    expect(needsProcessing(prefs)).toBe(true)
  })
})

describe('stripExifFromJpeg', () => {
  it('removes APP1 markers from JPEG data', () => {
    // Build a minimal JPEG with SOI + APP1(Exif) + EOI markers
    const soi = new Uint8Array([0xFF, 0xD8])
    const app1Length = 8 // total segment length including the 2 length bytes
    const app1 = new Uint8Array([0xFF, 0xE1, 0x00, app1Length, 0x45, 0x78, 0x69, 0x66]) // "Exif"
    const eoi = new Uint8Array([0xFF, 0xD9])
    const jpeg = new Uint8Array([...soi, ...app1, ...eoi])

    const result = stripExifFromJpeg(jpeg.buffer)
    const resultArr = new Uint8Array(result)

    // Should still have SOI at start and EOI at end
    expect(resultArr[0]).toBe(0xFF)
    expect(resultArr[1]).toBe(0xD8)
    expect(resultArr[resultArr.length - 2]).toBe(0xFF)
    expect(resultArr[resultArr.length - 1]).toBe(0xD9)

    // APP1 marker should be gone
    let hasApp1 = false
    for (let i = 0; i < resultArr.length - 1; i++) {
      if (resultArr[i] === 0xFF && resultArr[i + 1] === 0xE1) {
        hasApp1 = true
        break
      }
    }
    expect(hasApp1).toBe(false)

    // Result should be smaller (APP1 removed)
    expect(result.byteLength).toBeLessThan(jpeg.byteLength)
  })

  it('returns original data unchanged if no APP1 marker', () => {
    const soi = new Uint8Array([0xFF, 0xD8])
    const eoi = new Uint8Array([0xFF, 0xD9])
    const jpeg = new Uint8Array([...soi, ...eoi])

    const result = stripExifFromJpeg(jpeg.buffer)
    expect(new Uint8Array(result)).toEqual(jpeg)
  })

  it('returns data unchanged if not a JPEG (no SOI marker)', () => {
    const notJpeg = new Uint8Array([0x89, 0x50, 0x4E, 0x47]) // PNG signature
    const result = stripExifFromJpeg(notJpeg.buffer)
    expect(new Uint8Array(result)).toEqual(notJpeg)
  })

  it('handles very short buffers gracefully', () => {
    const short = new Uint8Array([0xFF])
    const result = stripExifFromJpeg(short.buffer)
    expect(new Uint8Array(result)).toEqual(short)
  })
})

describe('processFile', () => {
  it('returns original file unchanged when no processing needed', async () => {
    const file = new File(['dummy'], 'test.jpg', { type: 'image/jpeg' })
    const result = await processFile(file, DEFAULT_PREFS)
    // When no processing needed, should return same reference or equal content
    expect(result.name).toBe('test.jpg')
    expect(result.type).toBe('image/jpeg')
  })

  it('strips EXIF from a real JPEG when stripExif is enabled', async () => {
    // Build a test JPEG with APP1 marker
    const soi = new Uint8Array([0xFF, 0xD8])
    const app1Length = 10
    const app1 = new Uint8Array([0xFF, 0xE1, 0x00, app1Length, 0x45, 0x78, 0x69, 0x66, 0x00, 0x00])
    const eoi = new Uint8Array([0xFF, 0xD9])
    const jpegBytes = new Uint8Array([...soi, ...app1, ...eoi])
    const file = new File([jpegBytes], 'photo.jpg', { type: 'image/jpeg' })

    const prefs = { ...DEFAULT_PREFS, stripExif: true }
    const result = await processFile(file, prefs)

    expect(result.name).toBe('photo.jpg')
    expect(result.type).toBe('image/jpeg')
    // Result should be smaller (APP1 removed)
    expect(result.size).toBeLessThan(file.size)
  })
})
