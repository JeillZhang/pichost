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
 * Strip EXIF (APP1) markers from JPEG binary data.
 * Returns a new ArrayBuffer with APP1 segments removed.
 * Non-JPEG data is returned unchanged.
 */
export function stripExifFromJpeg(buffer: ArrayBuffer): ArrayBuffer {
  const bytes = new Uint8Array(buffer)
  // Must start with SOI marker (0xFFD8)
  if (bytes.length < 2 || bytes[0] !== 0xFF || bytes[1] !== 0xD8) {
    return buffer
  }

  const result: number[] = []
  let i = 0
  while (i < bytes.length) {
    if (i + 1 < bytes.length && bytes[i] === 0xFF) {
      const marker = bytes[i + 1]
      // APP1 (0xE1) — skip this entire segment
      if (marker === 0xE1) {
        if (i + 3 < bytes.length) {
          const segmentLength = (bytes[i + 2] << 8) | bytes[i + 3]
          i += 2 + segmentLength
          continue
        }
        i += 2
        continue
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

  // If the original buffer ends with EOI (FF D9) but the result doesn't
  // (which can happen when a segment length field extends past the actual
  // segment data into the EOI), append EOI to keep a valid JPEG structure.
  if (bytes.length >= 2
    && bytes[bytes.length - 2] === 0xFF
    && bytes[bytes.length - 1] === 0xD9
    && (result.length < 2
      || result[result.length - 2] !== 0xFF
      || result[result.length - 1] !== 0xD9)
  ) {
    result.push(0xFF, 0xD9)
  }

  return new Uint8Array(result).buffer
}

// Canvas-dependent operations (scaleDimensions, fileToImageBitmap, applyCanvasOperations)
// are implemented here but tested via E2E since they require a real Canvas/OffscreenCanvas.
// The pure logic functions (needsProcessing, stripExifFromJpeg) are fully unit-tested above.

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

/** Convert a File to ArrayBuffer, with jsdom fallback */
async function fileToArrayBuffer(file: File): Promise<ArrayBuffer> {
  // Blob.arrayBuffer() is available in browsers and jsdom 25+
  if (typeof file.arrayBuffer === 'function') {
    return file.arrayBuffer()
  }
  // Fallback for environments without Blob.arrayBuffer
  return new Response(file).arrayBuffer()
}

async function fileToImageBitmap(file: File): Promise<ImageBitmap> {
  return createImageBitmap(file)
}

async function applyCanvasOperations(
  source: ImageBitmap,
  prefs: PreprocessingPrefs,
  originalType: string,
): Promise<Blob> {
  let srcW = source.width
  let srcH = source.height

  const swapDimensions = prefs.rotate.enabled
    && (prefs.rotate.degrees === 90 || prefs.rotate.degrees === 270)

  let targetW = swapDimensions ? srcH : srcW
  let targetH = swapDimensions ? srcW : srcH

  if (prefs.resize.enabled) {
    const scaled = scaleDimensions(targetW, targetH, prefs.resize.maxWidth, prefs.resize.maxHeight)
    targetW = scaled.width
    targetH = scaled.height
  }

  let outputType = originalType
  if (prefs.formatConvert.enabled) {
    outputType = prefs.formatConvert.targetFormat
  }

  let quality: number | undefined
  if (prefs.formatConvert.enabled) {
    quality = prefs.formatConvert.quality / 100
  } else if (prefs.compression.enabled) {
    quality = prefs.compression.quality / 100
  }

  // Use OffscreenCanvas in Worker, regular canvas on main thread
  const CanvasClass = typeof OffscreenCanvas !== 'undefined'
    ? OffscreenCanvas
    : (() => { throw new Error('OffscreenCanvas not available') })()
  const canvas = new CanvasClass(targetW, targetH) as OffscreenCanvas
  const ctx = canvas.getContext('2d')!
  ctx.imageSmoothingEnabled = true
  ctx.imageSmoothingQuality = 'high'

  if (prefs.rotate.enabled && prefs.rotate.degrees !== 0) {
    ctx.save()
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

  const blob = await canvas.convertToBlob({
    type: outputType,
    quality,
  })

  source.close()
  return blob
}

/**
 * Main entry point: process a File according to preprocessing preferences.
 */
export async function processFile(
  file: File,
  prefs: PreprocessingPrefs,
): Promise<File> {
  if (!needsProcessing(prefs)) {
    return file
  }

  const dotIndex = file.name.lastIndexOf('.')
  const baseName = dotIndex > 0 ? file.name.substring(0, dotIndex) : file.name

  // Step 1: Strip EXIF from JPEG if requested
  let imageBuffer: ArrayBuffer = await fileToArrayBuffer(file)
  if (prefs.stripExif && file.type === 'image/jpeg') {
    imageBuffer = stripExifFromJpeg(imageBuffer)
  }

  // Step 2: If only stripExif was requested, return early
  const needsCanvas = prefs.resize.enabled
    || prefs.formatConvert.enabled
    || prefs.compression.enabled
    || prefs.rotate.enabled

  if (!needsCanvas) {
    return new File([imageBuffer], file.name, { type: file.type })
  }

  // Step 3: Canvas-based operations
  const sourceFile = new File([imageBuffer], file.name, { type: file.type })
  const bitmap = await fileToImageBitmap(sourceFile)
  const blob = await applyCanvasOperations(bitmap, prefs, file.type)

  let outputName: string
  if (prefs.formatConvert.enabled) {
    const ext = prefs.formatConvert.targetFormat.split('/')[1]
    outputName = `${baseName}.${ext}`
  } else {
    outputName = file.name
  }

  return new File([blob], outputName, { type: blob.type || file.type })
}
