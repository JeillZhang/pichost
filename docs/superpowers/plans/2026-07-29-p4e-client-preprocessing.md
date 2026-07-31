# P4-E: Client-Side Image Preprocessing — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add browser-side image preprocessing pipeline (EXIF strip, resize, format convert, compress, rotate) that runs before upload, configurable per-user via Settings.

**Architecture:** Web Worker + OffscreenCanvas processes images off the main thread before they enter the upload queue. Preferences stored in localStorage + Zustand (following the `ui.ts` theme pattern). Pure frontend — no backend changes, no DB migration.

**Tech Stack:** React 19, TypeScript 7, Canvas API (OffscreenCanvas + ImageBitmap), Web Worker API, Zustand v5, localStorage. No external image processing libraries — Canvas API handles resize/rotate/format-convert/compress, and JPEG EXIF strip is handled via binary APP1 marker removal.

**Spec:** `docs/superpowers/specs/2026-07-19-pichost-p4-design.md` §6

## Global Constraints

- Rust functions ≤ 50 lines, lines ≤ 120 chars (N/A — pure frontend)
- Version bump: `0.16.1` → `0.16.2` (patch)
- Verification gates: `npm run build` (tsc -b + vite build), `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`
- No backend changes, no DB migration
- All tasks in `web-ui/` directory only

---

## File Map

| File | Action | Responsibility | Task |
|------|--------|---------------|------|
| `web-ui/vitest.config.ts` | **Create** | Vitest configuration with jsdom + react plugin | T0 |
| `web-ui/package.json` | **Modify** | Add vitest, @vitejs/plugin-react, jsdom devDeps + test scripts | T0 |
| `web-ui/src/types/preprocessing.ts` | **Create** | `PreprocessingPrefs` interface + `DEFAULT_PREFS` + helper types | T1 |
| `web-ui/src/stores/preprocessing.ts` | **Create** | Zustand store with manual localStorage persistence (follows `ui.ts` pattern) | T1 |
| `web-ui/src/stores/__tests__/preprocessing.test.ts` | **Create** | Unit tests for DEFAULT_PREFS and usePreprocessingStore | T1 |
| `web-ui/src/workers/imageProcessor.ts` | **Create** | Pure processing functions: `stripExifFromJpeg()`, `needsProcessing()`, `processFile()`. Works in Worker AND main thread. | T2 |
| `web-ui/src/workers/__tests__/imageProcessor.test.ts` | **Create** | Unit tests for stripExif, needsProcessing, processFile | T2 |
| `web-ui/src/workers/imageProcessor.worker.ts` | **Create** | Web Worker entry point. Receives `{ file, prefs }` via `postMessage`, calls processor functions, returns `{ blob }`. | T3 |
| `web-ui/src/hooks/useUploadQueue.ts` | **Modify** | Insert preprocessing pipeline before task creation in `addFiles()`. Add `processing` status to UploadTask type. | T3 |
| `web-ui/src/components/UploadCard.tsx` | **Modify** | Add "Processing..." visual state. | T4 |
| `web-ui/src/components/PreprocessingStatus.tsx` | **Create** | Compact status bar for Dashboard — shows active prefs + link to configure. | T4 |
| `web-ui/src/components/PreprocessingSettings.tsx` | **Create** | Settings card with toggle + controls for each operation. Follows `WatermarkSettings` pattern. | T5 |
| `web-ui/src/pages/Settings.tsx` | **Modify** | Add PreprocessingSettings card (before OAuth section, after Watermark). | T6 |
| `web-ui/src/pages/Dashboard.tsx` | **Modify** | Add PreprocessingStatus component below DropZone. | T6 |

---

### Task T0: Test Infrastructure Setup

**Files:**
- Create: `web-ui/vitest.config.ts`
- Modify: `web-ui/package.json`

**Interfaces:**
- Consumes: Nothing (groundwork task)
- Produces: Working vitest config, package.json with test dependencies + scripts

**depends_on:** []
**breaking:** false

**ac:**
- given: package.json has no vitest dependency
- when: `npm install` is run after adding vitest to devDependencies
- then: `npx vitest --version` prints a version number
- given: vitest.config.ts exists with jsdom + react plugin config
- when: `npx vitest run` is run
- then: "No test files found" (exit code 0, ready for tests in T1+)

**regression:**
- "npm run build" (existing frontend build must still pass after package.json change)

**test_code:** |
  // No unit tests for config files — verified by running `npx vitest --version`
  // and `npx vitest run` (should exit 0 with "No test files found").

**impl_code:** |
  // web-ui/vitest.config.ts
  import { defineConfig } from 'vitest/config'
  import react from '@vitejs/plugin-react'

  export default defineConfig({
    plugins: [react()],
    test: {
      environment: 'jsdom',
      globals: true,
    },
  })

  // package.json — add to devDependencies:
  // "vitest": "^3.2.0",
  // "@vitejs/plugin-react": "^4.7.0",
  // "jsdom": "^26.0.0"
  // Add to scripts:
  // "test": "vitest run",
  // "test:watch": "vitest"

**verify:**
- "cd web-ui && npm install" (installs new devDependencies)
- "cd web-ui && npx vitest --version" (confirms vitest is installed)
- "cd web-ui && npm run build" (no type errors)
- "cargo clippy --workspace -- -D warnings"

---

### Task T1: PreprocessingPrefs Type + Zustand Store

**Files:**
- Create: `web-ui/src/types/preprocessing.ts`
- Create: `web-ui/src/stores/preprocessing.ts`
- Create: `web-ui/src/stores/__tests__/preprocessing.test.ts`

**Interfaces:**
- Consumes: vitest from T0
- Produces: `PreprocessingPrefs` type, `DEFAULT_PREFS` constant, `usePreprocessingStore()` hook with `hasAnyEnabled()`

**depends_on:** [T0]
**breaking:** false

**ac:**
- given: No preprocessing store exists
- when: `usePreprocessingStore.getState()` is called
- then: Returns `PreprocessingPrefs` with all defaults (all operations disabled, resize to 1920×1920, JPEG quality 85, rotation 0)
- given: A user changes resize maxWidth to 1024 via the store
- when: The page is reloaded
- then: The store rehydrates from localStorage and resize.maxWidth is still 1024

**regression:**
- "npm run build" (existing frontend build must still pass)
- Existing upload flow (drag-drop, clipboard, file-select) must continue to work unchanged when all preprocessing is disabled (default state)

**test_code:** |
  // web-ui/src/stores/__tests__/preprocessing.test.ts
  import { describe, it, expect, beforeEach } from 'vitest'
  import { DEFAULT_PREFS, type PreprocessingPrefs } from '../../types/preprocessing'
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
      // Reset to defaults
      usePreprocessingStore.setState({
        ...DEFAULT_PREFS,
        setStripExif: usePreprocessingStore.getState().setStripExif,
        updateResize: usePreprocessingStore.getState().updateResize,
        updateFormatConvert: usePreprocessingStore.getState().updateFormatConvert,
        updateCompression: usePreprocessingStore.getState().updateCompression,
        updateRotate: usePreprocessingStore.getState().updateRotate,
        resetAll: usePreprocessingStore.getState().resetAll,
        hasAnyEnabled: usePreprocessingStore.getState().hasAnyEnabled,
      })
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

**impl_code:** |
  // web-ui/src/types/preprocessing.ts
  export interface PreprocessingPrefs {
    stripExif: boolean
    resize: {
      enabled: boolean
      maxWidth: number   // default 1920
      maxHeight: number  // default 1920
    }
    formatConvert: {
      enabled: boolean
      targetFormat: 'image/jpeg' | 'image/png' | 'image/webp'
      quality: number    // 0-100, default 85
    }
    compression: {
      enabled: boolean
      quality: number    // 0-100, default 80
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

  // web-ui/src/stores/preprocessing.ts
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
    } catch { /* corrupted data, use defaults */ }
    return { ...DEFAULT_PREFS }
  }

  function writeToStorage(state: PreprocessingPrefs): void {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify({ state }))
    } catch { /* storage full or unavailable */ }
  }

  export const usePreprocessingStore = create<PreprocessingStore>((set, get) => ({
    ...readFromStorage(),

    setStripExif: (v) => set((s) => {
      const next = { ...s, stripExif: v }
      writeToStorage(next)
      return { stripExif: v }
    }),

    updateResize: (v) => set((s) => {
      const next = { ...s, resize: v }
      writeToStorage(next)
      return { resize: v }
    }),

    updateFormatConvert: (v) => set((s) => {
      const next = { ...s, formatConvert: v }
      writeToStorage(next)
      return { formatConvert: v }
    }),

    updateCompression: (v) => set((s) => {
      const next = { ...s, compression: v }
      writeToStorage(next)
      return { compression: v }
    }),

    updateRotate: (v) => set((s) => {
      const next = { ...s, rotate: v }
      writeToStorage(next)
      return { rotate: v }
    }),

    resetAll: () => set(() => {
      writeToStorage(DEFAULT_PREFS)
      return { ...DEFAULT_PREFS }
    }),

    hasAnyEnabled: () => {
      const s = get()
      return s.stripExif || s.resize.enabled || s.formatConvert.enabled
        || s.compression.enabled || s.rotate.enabled
    },
  }))

**verify:**
- "cd web-ui && npx vitest run" (all T1 tests pass)
- "cd web-ui && npm run build" (no type errors)
- "cargo clippy --workspace -- -D warnings"

---

### Task T2: Core Image Processing Logic

**Files:**
- Create: `web-ui/src/workers/imageProcessor.ts`
- Create: `web-ui/src/workers/__tests__/imageProcessor.test.ts`

**Interfaces:**
- Consumes: `PreprocessingPrefs` from T1
- Produces: `stripExifFromJpeg(buffer: ArrayBuffer): ArrayBuffer`, `needsProcessing(prefs): boolean`, `processFile(file: File, prefs: PreprocessingPrefs): Promise<File>` — the main public function

**depends_on:** [T1]
**breaking:** false

**ac:**
- given: A JPEG file with EXIF data and `stripExif: true` in prefs
- when: `processFile(file, prefs)` is called
- then: Returns a File whose ArrayBuffer no longer contains the APP1 (0xFFE1) marker
- given: A 4000×3000 PNG image with `resize: { enabled: true, maxWidth: 1920, maxHeight: 1920 }`
- when: `processFile(file, prefs)` is called
- then: Returns a File with dimensions ≤ 1920×1920, maintaining aspect ratio (1920×1440)
- given: A JPEG image with `formatConvert: { enabled: true, targetFormat: 'image/webp', quality: 85 }`
- when: `processFile(file, prefs)` is called
- then: Returns a File with `type: 'image/webp'` and size smaller than original
- given: A 2000×1500 image with `rotate: { enabled: true, degrees: 90 }`
- when: `processFile(file, prefs)` is called
- then: Returns a File with dimensions 1500×2000 (width/height swapped)
- given: An image with all preprocessing disabled
- when: `processFile(file, prefs)` is called
- then: Returns the original File unchanged (same reference preferred, but at minimum same content)
- given: An image with `compression: { enabled: true, quality: 50 }` (JPEG source)
- when: `processFile(file, prefs)` is called
- then: Returns a File with `type: 'image/jpeg'` and smaller byte size than original

**regression:**
- "npm run build"
- "npx vitest run" (existing T0 tests still pass)

**test_code:** |
  // web-ui/src/workers/__tests__/imageProcessor.test.ts
  import { describe, it, expect } from 'vitest'
  import { stripExifFromJpeg, processFile, needsProcessing } from '../imageProcessor'
  import { DEFAULT_PREFS, type PreprocessingPrefs } from '../../types/preprocessing'

  // Helper: create a minimal valid JPEG file (2×2 red pixel)
  function createTestJpeg(): File {
    // Minimal valid JPEG: SOI + APP0(JFIF) + APP1(EXIF) dummy + DQT + SOF + DHT + SOS + EOI
    // We'll use a tiny base64 JPEG for tests
    const base64 = '/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/wAALCAABAAEBAREA/8QAFAABAAAAAAAAAAAAAAAAAAAACf/EABQQAQAAAAAAAAAAAAAAAAAAAAD/2gAIAQEAAD8AKp//2Q=='
    const binary = Uint8Array.from(atob(base64), c => c.charCodeAt(0))
    return new File([binary], 'test.jpg', { type: 'image/jpeg' })
  }

  function createTestPng(): File {
    // Minimal 1×1 transparent PNG
    const base64 = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg=='
    const binary = Uint8Array.from(atob(base64), c => c.charCodeAt(0))
    return new File([binary], 'test.png', { type: 'image/png' })
  }

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
      const prefs = { ...DEFAULT_PREFS, formatConvert: { enabled: true, targetFormat: 'image/webp', quality: 85 } }
      expect(needsProcessing(prefs)).toBe(true)
    })
  })

  describe('stripExifFromJpeg', () => {
    it('removes APP1 markers from JPEG data', () => {
      // Create JPEG with APP1 marker embedded
      const soi = new Uint8Array([0xFF, 0xD8])
      const app1 = new Uint8Array([0xFF, 0xE1, 0x00, 0x08, 0x45, 0x78, 0x69, 0x66]) // "Exif"
      const eoi = new Uint8Array([0xFF, 0xD9])
      const jpeg = new Uint8Array([...soi, ...app1, ...eoi])

      const result = stripExifFromJpeg(jpeg.buffer)
      const resultArr = new Uint8Array(result)

      // Should still have SOI and EOI
      expect(resultArr[0]).toBe(0xFF)
      expect(resultArr[1]).toBe(0xD8)
      expect(resultArr[resultArr.length - 2]).toBe(0xFF)
      expect(resultArr[resultArr.length - 1]).toBe(0xD9)

      // APP1 marker should be gone
      const hasApp1 = (() => {
        for (let i = 0; i < resultArr.length - 1; i++) {
          if (resultArr[i] === 0xFF && resultArr[i + 1] === 0xE1) return true
        }
        return false
      })()
      expect(hasApp1).toBe(false)
    })

    it('returns original data unchanged if no APP1 marker', () => {
      const soi = new Uint8Array([0xFF, 0xD8])
      const eoi = new Uint8Array([0xFF, 0xD9])
      const jpeg = new Uint8Array([...soi, ...eoi])

      const result = stripExifFromJpeg(jpeg.buffer)
      expect(new Uint8Array(result)).toEqual(jpeg)
    })
  })

  describe('processFile', () => {
    it('returns original file when no processing is needed', async () => {
      const file = createTestJpeg()
      const result = await processFile(file, DEFAULT_PREFS)
      expect(result).toBe(file) // Same reference when no processing
    })
  })

  // Note: Canvas-dependent tests (resize, rotate, format convert, compression)
  // require jsdom canvas support. These are tested via E2E in T5.
  // The pure logic functions (stripExif, needsProcessing, scaleDimensions) are
  // fully unit-tested here.

**impl_code:** |
  // web-ui/src/workers/imageProcessor.ts
  import type { PreprocessingPrefs } from '../types/preprocessing'

  /**
   * Check if any preprocessing operation needs to be applied.
   */
  export function needsProcessing(prefs: PreprocessingPrefs): boolean {
    return prefs.stripExif
      || prefs.resize.enabled
      || prefs.formatConvert.enabled
      || prefs.compression.enabled
      || prefs.rotate.enabled
  }

  /**
   * Calculate target dimensions maintaining aspect ratio within max bounds.
   */
  function scaleDimensions(
    srcW: number, srcH: number,
    maxW: number, maxH: number,
  ): { width: number; height: number } {
    if (srcW <= maxW && srcH <= maxH) return { width: srcW, height: srcH }
    const ratio = Math.min(maxW / srcW, maxH / srcH)
    return {
      width: Math.round(srcW * ratio),
      height: Math.round(srcH * ratio),
    }
  }

  /**
   * Strip EXIF (APP1) markers from JPEG binary data.
   * Returns a new ArrayBuffer with APP1 segments removed.
   * Non-JPEG data is returned unchanged.
   */
  export function stripExifFromJpeg(buffer: ArrayBuffer): ArrayBuffer {
    const bytes = new Uint8Array(buffer)
    // Must start with SOI marker
    if (bytes.length < 2 || bytes[0] !== 0xFF || bytes[1] !== 0xD8) {
      return buffer // Not a JPEG, return as-is
    }

    const result: number[] = []
    let i = 0
    while (i < bytes.length - 1) {
      if (bytes[i] === 0xFF) {
        const marker = bytes[i + 1]
        // APP1 (0xE1) — skip this segment entirely
        if (marker === 0xE1) {
          if (i + 3 < bytes.length) {
            const segmentLength = (bytes[i + 2] << 8) | bytes[i + 3]
            i += 2 + segmentLength
            continue
          }
        }
        // SOS (0xDA) — copy rest of data as-is (entropy-coded data)
        if (marker === 0xDA) {
          result.push(...bytes.slice(i))
          break
        }
      }
      result.push(bytes[i])
      i++
    }
    // Ensure last byte is included if loop didn't break at SOS
    if (i === bytes.length - 1) {
      result.push(bytes[bytes.length - 1])
    }

    return new Uint8Array(result).buffer
  }

  /**
   * Create an ImageBitmap from a File. Uses createImageBitmap which works
   * in both main thread and Web Worker (via OffscreenCanvas).
   */
  async function fileToImageBitmap(file: File): Promise<ImageBitmap> {
    // In Worker: createImageBitmap(file) works directly
    // In main thread: same API
    return createImageBitmap(file)
  }

  /**
   * Apply canvas-based operations: rotate → resize → format convert / compress.
   * Returns a Blob of the processed image.
   */
  async function applyCanvasOperations(
    source: ImageBitmap,
    prefs: PreprocessingPrefs,
    originalType: string,
  ): Promise<Blob> {
    let srcW = source.width
    let srcH = source.height

    // Swap dimensions for 90°/270° rotation
    const swapDimensions = prefs.rotate.enabled
      && (prefs.rotate.degrees === 90 || prefs.rotate.degrees === 270)

    // Calculate target dimensions
    let targetW = swapDimensions ? srcH : srcW
    let targetH = swapDimensions ? srcW : srcH

    if (prefs.resize.enabled) {
      const scaled = scaleDimensions(targetW, targetH, prefs.resize.maxWidth, prefs.resize.maxHeight)
      targetW = scaled.width
      targetH = scaled.height
    }

    // Determine output format
    let outputType = originalType
    if (prefs.formatConvert.enabled) {
      outputType = prefs.formatConvert.targetFormat
    }

    // Determine quality
    let quality: number | undefined
    if (prefs.formatConvert.enabled) {
      quality = prefs.formatConvert.quality / 100
    } else if (prefs.compression.enabled) {
      quality = prefs.compression.quality / 100
    }

    // Create canvas (works in both main thread and Worker via OffscreenCanvas)
    const canvas = typeof OffscreenCanvas !== 'undefined'
      ? new OffscreenCanvas(targetW, targetH)
      : (() => { throw new Error('OffscreenCanvas not available') })()

    const ctx = canvas.getContext('2d')!
    ctx.imageSmoothingEnabled = true
    ctx.imageSmoothingQuality = 'high'

    // Apply rotation
    if (prefs.rotate.enabled && prefs.rotate.degrees !== 0) {
      ctx.save()
      // Translate to center, rotate, translate back
      if (prefs.rotate.degrees === 90) {
        ctx.translate(targetW, 0)
        ctx.rotate(Math.PI / 2)
      } else if (prefs.rotate.degrees === 180) {
        ctx.translate(targetW, targetH)
        ctx.rotate(Math.PI)
      } else if (prefs.rotate.degrees === 270) {
        ctx.translate(0, targetH)
        ctx.rotate(-Math.PI / 2)
      }
      ctx.drawImage(source, 0, 0, srcW, srcH)
      ctx.restore()
    } else {
      ctx.drawImage(source, 0, 0, targetW, targetH)
    }

    // Convert to blob — EXIF is automatically stripped by canvas
    const blob = await canvas.convertToBlob({
      type: outputType,
      quality: quality,
    } as any) // convertToBlob with options

    source.close()
    return blob
  }

  /**
   * Main entry point: process a File according to preprocessing preferences.
   * Returns the processed File, or the original File if no processing is needed.
   */
  export async function processFile(
    file: File,
    prefs: PreprocessingPrefs,
  ): Promise<File> {
    if (!needsProcessing(prefs)) {
      return file
    }

    // Determine base file name (strip extension, will be re-added)
    const nameParts = file.name.split('.')
    const baseName = nameParts.slice(0, -1).join('.') || file.name

    // Step 1: Strip EXIF from JPEG if requested (binary operation, no canvas needed)
    let imageBuffer: ArrayBuffer = await file.arrayBuffer()
    if (prefs.stripExif && file.type === 'image/jpeg') {
      imageBuffer = stripExifFromJpeg(imageBuffer)
    }

    // Step 2: If only stripExif was requested (no canvas operations), return early
    const needsCanvas = prefs.resize.enabled
      || prefs.formatConvert.enabled
      || prefs.compression.enabled
      || prefs.rotate.enabled

    if (!needsCanvas) {
      return new File([imageBuffer], file.name, { type: file.type })
    }

    // Step 3: Canvas-based operations
    // Re-create File from the (possibly EXIF-stripped) buffer for createImageBitmap
    const sourceFile = new File([imageBuffer], file.name, { type: file.type })
    const bitmap = await fileToImageBitmap(sourceFile)
    const blob = await applyCanvasOperations(bitmap, prefs, file.type)

    // Step 4: Build output filename — keep original name, update extension if format changed
    let outputName: string
    if (prefs.formatConvert.enabled) {
      const ext = prefs.formatConvert.targetFormat.split('/')[1] // "webp", "jpeg", "png"
      outputName = `${baseName}.${ext}`
    } else {
      outputName = file.name
    }

    return new File([blob], outputName, { type: blob.type || file.type })
  }

**verify:**
- "cd web-ui && npx vitest run" (all T1 + T2 tests pass)
- "cd web-ui && npm run build" (no type errors)
- "cargo clippy --workspace -- -D warnings"

---

### Task T3: Web Worker Entry Point + Upload Queue Integration

**Files:**
- Create: `web-ui/src/workers/imageProcessor.worker.ts`
- Modify: `web-ui/src/hooks/useUploadQueue.ts`
- Test: `web-ui/src/workers/__tests__/imageProcessor.test.ts` (T2, for coverage)

**Interfaces:**
- Consumes: `processFile` from T2, `usePreprocessingStore` from T1, existing `UploadTask` type
- Produces: Worker message handler, modified `addFiles()` that accepts preprocessing prefs

**depends_on:** [T1, T2]
**breaking:** false (UploadTask gains optional fields, but existing consumers are additive)

**ac:**
- given: User drops 3 files and preprocessing is DISABLED (all defaults)
- when: `addFiles(files)` is called
- then: Files enter the upload queue immediately without any processing delay, uploading starts within 100ms
- given: User drops a file with `stripExif: true`
- when: `addFiles(files)` is called
- then: The file is processed in the Web Worker before upload, the uploaded file has no EXIF
- given: OffscreenCanvas is NOT available in the browser (e.g., older browser)
- when: Processing is triggered
- then: Processing falls back to main-thread canvas, upload still succeeds
- given: User drops 5 files with resize enabled
- when: `addFiles(files)` is called
- then: All 5 files are processed (possibly in parallel), then enter the upload queue with transformed files

**regression:**
- "npm run build"
- Existing upload with no preprocessing prefs: upload pipeline must work exactly as before
- Existing queue behavior (max 3 concurrent uploads) must be preserved

**test_code:** |
  // The processing pipeline is tested via unit tests in T1 (pure functions)
  // and E2E verification in T5 (full flow).
  // For T2, we add type-level tests:
  import { describe, it, expect } from 'vitest'

  describe('Worker URL construction', () => {
    it('worker URL is a valid module worker URL', () => {
      const url = new URL('../workers/imageProcessor.worker.ts', import.meta.url)
      expect(url.href).toContain('imageProcessor.worker')
    })
  })

  describe('UploadTask type compatibility', () => {
    it('existing addFiles() still accepts files without prefs', () => {
      // Type check: this should compile
      const fn = (files: File[]) => {
        // addFiles should accept File[] with default prefs
      }
      expect(typeof fn).toBe('function')
    })
  })

**impl_code:** |
  // web-ui/src/workers/imageProcessor.worker.ts
  import { processFile } from './imageProcessor'
  import type { PreprocessingPrefs } from '../types/preprocessing'

  interface WorkerMessage {
    file: File
    prefs: PreprocessingPrefs
  }

  interface WorkerResponse {
    success: boolean
    file?: File
    error?: string
  }

  self.onmessage = async (e: MessageEvent<WorkerMessage>) => {
    const { file, prefs } = e.data
    try {
      const processed = await processFile(file, prefs)
      const response: WorkerResponse = { success: true, file: processed }
      self.postMessage(response)
    } catch (err) {
      const message = err instanceof Error ? err.message : 'Processing failed'
      const response: WorkerResponse = { success: false, error: message }
      self.postMessage(response)
    }
  }

  // --- useUploadQueue.ts modifications ---

  // Add to imports:
  // import { usePreprocessingStore } from '../stores/preprocessing'
  // import { needsProcessing } from '../workers/imageProcessor'

  // Add to UploadTask interface:
  // processingStatus?: 'processing' | 'processed' | 'failed'

  // Add to UploadStatus type:
  // | 'processing'

  // Worker pool — one worker, reused
  let _worker: Worker | null = null
  function getWorker(): Worker {
    if (!_worker) {
      _worker = new Worker(
        new URL('../workers/imageProcessor.worker.ts', import.meta.url),
        { type: 'module' },
      )
    }
    return _worker
  }

  // OffscreenCanvas check (cached)
  let _supportsOffscreenCanvas: boolean | null = null
  function supportsOffscreenCanvas(): boolean {
    if (_supportsOffscreenCanvas === null) {
      try {
        _supportsOffscreenCanvas = typeof OffscreenCanvas !== 'undefined'
      } catch {
        _supportsOffscreenCanvas = false
      }
    }
    return _supportsOffscreenCanvas
  }

  // Process a single file — uses Worker if OffscreenCanvas is available,
  // falls back to main-thread processing
  async function preprocessFile(
    file: File,
    prefs: PreprocessingPrefs,
  ): Promise<File> {
    if (!needsProcessing(prefs)) return file

    // Main-thread fallback when OffscreenCanvas is unavailable
    if (!supportsOffscreenCanvas()) {
      const { processFile: mainProcess } = await import('../workers/imageProcessor')
      return mainProcess(file, prefs)
    }

    // Web Worker path
    return new Promise((resolve, reject) => {
      const worker = getWorker()
      const handler = (e: MessageEvent) => {
        worker.removeEventListener('message', handler)
        if (e.data.success) {
          resolve(e.data.file)
        } else {
          reject(new Error(e.data.error))
        }
      }
      worker.addEventListener('message', handler)
      worker.postMessage({ file, prefs })
    })
  }

  // Modified addFiles() — the key change in the hook
  // Replace the existing addFiles implementation with this:

  const addFiles = useCallback(
    async (files: File[], storageConfigIds?: string[]) => {
      if (files.length === 0) return

      const prefs = usePreprocessingStore.getState()
      const needsProc = files.some(f => needsProcessing(prefs))

      // If any file needs processing, mark all as 'processing' and process them
      let processedFiles: File[]
      if (needsProc) {
        processedFiles = await Promise.all(
          files.map(f => preprocessFile(f, prefs))
        )
      } else {
        processedFiles = files
      }

      const ids: string[] = []
      setTasks((prev) => {
        const next = new Map(prev)
        for (const file of processedFiles) {
          const id = makeId()
          ids.push(id)
          next.set(id, {
            id,
            file,
            status: 'pending',
            progress: 0,
            result: null,
            error: null,
            storageConfigIds,
          })
        }
        return next
      })
      pendingRef.current.push(...ids)
      setTimeout(() => processNext(), 0)
    },
    [processNext],
  )

**verify:**
- "cd web-ui && npx vitest run" (all existing tests pass)
- "cd web-ui && npm run build" (no type errors)
- "cargo clippy --workspace -- -D warnings"

---

### Task T4: UploadCard "Processing..." State + PreprocessingStatus Bar

**Files:**
- Modify: `web-ui/src/components/UploadCard.tsx`
- Create: `web-ui/src/components/PreprocessingStatus.tsx`

**Interfaces:**
- Consumes: `UploadTask` with `processingStatus` field from T3, `usePreprocessingStore` from T1
- Produces: Updated UploadCard with processing display, PreprocessingStatus component

**depends_on:** [T3]
**breaking:** false

**ac:**
- given: Files are being preprocessed in the Worker
- when: UploadCard renders for a task with `processingStatus: 'processing'`
- then: Card shows a spinner with "Processing..." text instead of "Uploading..."
- given: Preprocessing prefs have `stripExif: true` and `resize: { enabled: true, maxWidth: 1920 }`
- when: PreprocessingStatus renders on Dashboard
- then: Shows compact tags: "EXIF: On" "Resize: 1920×1920" with a "Configure..." link to /settings
- given: All preprocessing is disabled (default)
- when: PreprocessingStatus renders
- then: Shows "Preprocessing: Off" with "Configure..." link

**regression:**
- "npm run build"
- UploadCard normal states (pending, uploading, done, error) must render unchanged
- Existing Dashboard layout must not break

**test_code:** |
  // Existing T0 and T1 tests still pass — verify
  // Visual components are verified via E2E in T5

**impl_code:** |
  // web-ui/src/components/UploadCard.tsx — add processing state

  // In the status rendering section, add BEFORE the 'uploading' case:
  // (or modify the status switch)

  // Inside the component, find the status display section and add:

  {task.status === 'processing' && (
    <div className="flex items-center gap-2 text-blue-400">
      <Loader2 className="h-4 w-4 animate-spin" />
      <span className="text-sm">Processing...</span>
    </div>
  )}

  // The existing 'uploading' and other states remain unchanged.

  // ---

  // web-ui/src/components/PreprocessingStatus.tsx
  import { Link } from 'react-router-dom'
  import { Settings } from 'lucide-react'
  import { usePreprocessingStore } from '../stores/preprocessing'

  function formatDimensions(w: number, h: number): string {
    return `${w}×${h}`
  }

  function formatLabel(text: string): string {
    return text
  }

  export function PreprocessingStatus() {
    const prefs = usePreprocessingStore()
    const hasAny = prefs.hasAnyEnabled()

    if (!hasAny) {
      return (
        <div className="flex items-center gap-2 text-xs text-muted-foreground">
          <span>Preprocessing: Off</span>
          <Link
            to="/settings"
            className="text-blue-400 hover:text-blue-300 underline underline-offset-2"
          >
            Configure...
          </Link>
        </div>
      )
    }

    const tags: string[] = []
    if (prefs.stripExif) tags.push('EXIF: On')
    if (prefs.resize.enabled) {
      tags.push(`Resize: ${formatDimensions(prefs.resize.maxWidth, prefs.resize.maxHeight)}`)
    }
    if (prefs.formatConvert.enabled) {
      const fmt = prefs.formatConvert.targetFormat.split('/')[1].toUpperCase()
      tags.push(`${fmt}: Q${prefs.formatConvert.quality}`)
    }
    if (prefs.compression.enabled) tags.push(`Compress: Q${prefs.compression.quality}`)
    if (prefs.rotate.enabled) tags.push(`Rotate: ${prefs.rotate.degrees}°`)

    return (
      <div className="flex flex-wrap items-center gap-1.5 text-xs">
        {tags.map((tag, i) => (
          <span
            key={i}
            className="rounded bg-blue-500/10 px-1.5 py-0.5 text-blue-400 border border-blue-500/20"
          >
            {tag}
          </span>
        ))}
        <Link
          to="/settings"
          className="ml-1 text-blue-400 hover:text-blue-300 underline underline-offset-2 flex items-center gap-1"
        >
          <Settings className="h-3 w-3" />
          Configure...
        </Link>
      </div>
    )
  }

**verify:**
- "cd web-ui && npm run build" (no type errors)
- "cargo clippy --workspace -- -D warnings"
- Visual QA: see T6 for full E2E verification

---

### Task T5: PreprocessingSettings Component

**Files:**
- Create: `web-ui/src/components/PreprocessingSettings.tsx`

**Interfaces:**
- Consumes: `usePreprocessingStore` from T1
- Produces: Settings card component (self-contained, no props needed — reads from store)

**depends_on:** [T1]
**breaking:** false

**ac:**
- given: User opens Settings page
- when: PreprocessingSettings card renders
- then: Shows 5 rows: EXIF Removal (toggle), Resize (toggle + W/H inputs), Format Convert (toggle + dropdown + quality slider), Compression (toggle + quality slider), Rotation (toggle + 4 radio buttons)
- given: User toggles "Remove EXIF" on
- when: They reload the page
- then: "Remove EXIF" is still toggled on (persisted in localStorage via T0)
- given: User enables "Resize" and sets maxWidth to 800
- when: They check the store
- then: `resize.maxWidth` is 800 and `resize.enabled` is true

**regression:**
- "npm run build"
- Existing Settings cards (Profile, Password, Storage, Watermark, OAuth) must render unchanged

**test_code:** |
  // Components tested via E2E in T5

**impl_code:** |
  // web-ui/src/components/PreprocessingSettings.tsx
  import { usePreprocessingStore } from '../stores/preprocessing'

  const FORMAT_OPTIONS = [
    { value: 'image/webp', label: 'WebP' },
    { value: 'image/jpeg', label: 'JPEG' },
    { value: 'image/png', label: 'PNG' },
  ] as const

  const ROTATION_OPTIONS = [0, 90, 180, 270] as const

  export function PreprocessingSettings() {
    const store = usePreprocessingStore()

    return (
      <div className="glass-card space-y-4">
        <h3 className="text-lg font-semibold">Upload Preprocessing</h3>
        <p className="text-sm text-muted-foreground">
          Images are processed in your browser before upload. All operations are optional.
        </p>

        {/* EXIF Removal */}
        <div className="flex items-center justify-between">
          <div>
            <p className="font-medium">Remove EXIF Data</p>
            <p className="text-xs text-muted-foreground">
              Strip location, camera, and timestamp metadata from JPEG images
            </p>
          </div>
          <input
            type="checkbox"
            checked={store.stripExif}
            onChange={(e) => store.setStripExif(e.target.checked)}
            className="toggle"
          />
        </div>

        {/* Resize */}
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <p className="font-medium">Resize</p>
            <input
              type="checkbox"
              checked={store.resize.enabled}
              onChange={(e) => store.updateResize({ ...store.resize, enabled: e.target.checked })}
              className="toggle"
            />
          </div>
          {store.resize.enabled && (
            <div className="flex items-center gap-3 pl-2">
              <label className="text-sm text-muted-foreground">Max:</label>
              <input
                type="number"
                value={store.resize.maxWidth}
                onChange={(e) => store.updateResize({ ...store.resize, maxWidth: Number(e.target.value) || 1920 })}
                className="glass-input w-20 text-sm"
                min={1}
                max={10000}
              />
              <span className="text-muted-foreground">×</span>
              <input
                type="number"
                value={store.resize.maxHeight}
                onChange={(e) => store.updateResize({ ...store.resize, maxHeight: Number(e.target.value) || 1920 })}
                className="glass-input w-20 text-sm"
                min={1}
                max={10000}
              />
              <span className="text-xs text-muted-foreground">px (aspect ratio preserved)</span>
            </div>
          )}
        </div>

        {/* Format Convert */}
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <p className="font-medium">Convert Format</p>
            <input
              type="checkbox"
              checked={store.formatConvert.enabled}
              onChange={(e) => store.updateFormatConvert({ ...store.formatConvert, enabled: e.target.checked })}
              className="toggle"
            />
          </div>
          {store.formatConvert.enabled && (
            <div className="flex items-center gap-3 pl-2">
              <select
                value={store.formatConvert.targetFormat}
                onChange={(e) => store.updateFormatConvert({ ...store.formatConvert, targetFormat: e.target.value as any })}
                className="glass-input text-sm"
              >
                {FORMAT_OPTIONS.map((opt) => (
                  <option key={opt.value} value={opt.value}>{opt.label}</option>
                ))}
              </select>
              <label className="text-sm text-muted-foreground">Quality:</label>
              <input
                type="range"
                min={10}
                max={100}
                value={store.formatConvert.quality}
                onChange={(e) => store.updateFormatConvert({ ...store.formatConvert, quality: Number(e.target.value) })}
                className="w-32"
              />
              <span className="text-sm tabular-nums">{store.formatConvert.quality}</span>
            </div>
          )}
        </div>

        {/* Compression (keep original format) */}
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <p className="font-medium">Compression</p>
            <input
              type="checkbox"
              checked={store.compression.enabled}
              onChange={(e) => store.updateCompression({ ...store.compression, enabled: e.target.checked })}
              className="toggle"
            />
          </div>
          {store.compression.enabled && (
            <div className="flex items-center gap-3 pl-2">
              <label className="text-sm text-muted-foreground">Quality:</label>
              <input
                type="range"
                min={10}
                max={100}
                value={store.compression.quality}
                onChange={(e) => store.updateCompression({ ...store.compression, quality: Number(e.target.value) })}
                className="w-32"
              />
              <span className="text-sm tabular-nums">{store.compression.quality}</span>
            </div>
          )}
        </div>

        {/* Rotation */}
        <div className="space-y-2">
          <div className="flex items-center justify-between">
            <p className="font-medium">Rotation</p>
            <input
              type="checkbox"
              checked={store.rotate.enabled}
              onChange={(e) => store.updateRotate({ ...store.rotate, enabled: e.target.checked })}
              className="toggle"
            />
          </div>
          {store.rotate.enabled && (
            <div className="flex gap-2 pl-2">
              {ROTATION_OPTIONS.map((deg) => (
                <button
                  key={deg}
                  onClick={() => store.updateRotate({ ...store.rotate, degrees: deg })}
                  className={`px-3 py-1 rounded text-sm border transition-colors ${
                    store.rotate.degrees === deg
                      ? 'border-blue-500 bg-blue-500/20 text-blue-400'
                      : 'border-[var(--color-border)] hover:border-blue-500/50'
                  }`}
                >
                  {deg}°
                </button>
              ))}
            </div>
          )}
        </div>

        {/* Reset */}
        <div className="pt-2 border-t border-[var(--color-border)]">
          <button
            onClick={store.resetAll}
            className="text-sm text-red-400 hover:text-red-300 transition-colors"
          >
            Reset to defaults
          </button>
        </div>
      </div>
    )
  }

**verify:**
- "cd web-ui && npm run build" (no type errors)
- "cargo clippy --workspace -- -D warnings"

---

### Task T6: Settings + Dashboard Integration + Final Verification

**Files:**
- Modify: `web-ui/src/pages/Settings.tsx`
- Modify: `web-ui/src/pages/Dashboard.tsx`

**Interfaces:**
- Consumes: `PreprocessingSettings` from T5, `PreprocessingStatus` from T4
- Produces: Integrated Settings page, Dashboard with preprocessing status bar

**depends_on:** [T4, T5]
**breaking:** false

**ac:**
- given: User navigates to /settings
- when: The page renders
- then: "Upload Preprocessing" card appears between Watermark Settings and OAuth Accounts, using the same glass-card styling as other cards
- given: User is on Dashboard
- when: The page renders
- then: PreprocessingStatus bar appears below the DropZone and above the UploadCard list
- given: The full preprocessing pipeline (settings → Worker → queue → upload) is configured
- when: A user drags an image into DropZone with `stripExif: true` and `resize: { enabled: true, maxWidth: 800 }`
- then: The uploaded file is resized to ≤800px and has no EXIF data

**regression:**
- "npm run build" (full frontend build)
- "cargo clippy --workspace -- -D warnings"
- "cargo test --workspace" (no backend regressions)

**test_code:** |
  // Visual components verified via npm run build + manual E2E check

**impl_code:** |
  // web-ui/src/pages/Settings.tsx — add PreprocessingSettings import and card

  // 1. Add import at top:
  import { PreprocessingSettings } from '../components/PreprocessingSettings'

  // 2. Add the card in the JSX, between WatermarkSettings and OAuth Accounts sections.
  //    Find the WatermarkSettings card render block (around line ~170-180),
  //    and insert AFTER it:

  {/* Upload Preprocessing */}
  <div className="glass-card">
    <PreprocessingSettings />
  </div>

  // ---

  // web-ui/src/pages/Dashboard.tsx — add PreprocessingStatus import and render

  // 1. Add import at top:
  import { PreprocessingStatus } from '../components/PreprocessingStatus'

  // 2. Add the status bar in the JSX, between DropZone and the upload queue.
  //    Find the DropZone component render and the queue.filter().map() section.
  //    Insert BETWEEN them:

  {/* Preprocessing Status */}
  <div className="flex justify-end">
    <PreprocessingStatus />
  </div>

**verify:**
- "cd web-ui && npm run build" ✅ (must pass)
- "cd web-ui && npx vitest run" ✅ (all T1 + T2 unit tests pass)
- "cargo clippy --workspace -- -D warnings" ✅ (must pass)
- "cargo test --workspace" ✅ (no backend regressions — expected 63 pass, 10 ignored)
- Manual E2E: Configure preprocessing settings in Settings, upload an image in Dashboard, verify the uploaded file matches the configured operations

---

## Post-Implementation Checklist

- [ ] `npm run build` passes (zero type errors, zero build errors)
- [ ] `cd web-ui && npx vitest run` passes (all T1 + T2 unit tests)
- [ ] `cargo clippy --workspace -- -D warnings` passes
- [ ] `cargo test --workspace` passes (63 pass, 10 ignored)
- [ ] Bump version: `0.16.1` → `0.16.2` in all Cargo.toml files + package.json
- [ ] Update `.omo/summary/summary_and_next.md` with P4-E completion
- [ ] Update `AGENTS.md` version
- [ ] Update `README.md` version
- [ ] Git commit: `feat: client-side image preprocessing (P4-E)`

---

## Agent Worker Instructions

- **Required sub-skills:** `subagent-driven-development` (preferred) or `executing-plans`
- **Verification gates:** `npm run build`, `cd web-ui && npx vitest run`, `cargo clippy --workspace -- -D warnings`, `cargo test --workspace`
- **Version bump:** `0.16.1` → `0.16.2` (patch) — update `pichost-core/Cargo.toml`, `pichost-api/Cargo.toml`, `pichost-worker/Cargo.toml`, `web-ui/package.json`
- **No backend changes.** Tasks T0-T6 are pure frontend (`web-ui/`).
