import { useCallback, type ChangeEvent } from 'react'
import { useDropzone, type DropEvent } from 'react-dropzone'
import { Upload } from 'lucide-react'

/** Custom getFilesFromEvent to bypass file-selector's broken getAsFileSystemHandle()
 *  path in secure contexts (localhost). getAsFileSystemHandle() returns null for
 *  OS-dragged files, causing silent failures.
 *  Also fixes React 19 SyntheticEvent wrapping: use dataTransfer property directly
 *  instead of instanceof DragEvent (React wraps native events). */
const getFilesFromEvent = async (
  event: DropEvent,
): Promise<(DataTransferItem | File)[]> => {
  const dt = ('dataTransfer' in event ? event.dataTransfer : null) as DataTransfer | null
  if (dt?.files?.length) {
    const files: File[] = []
    for (let i = 0; i < dt.files.length; i++) files.push(dt.files[i])
    return files
  }
  const input = (event as ChangeEvent<HTMLElement>).target as HTMLInputElement | null
  if (input?.files?.length) {
    const files: File[] = []
    for (let i = 0; i < input.files!.length; i++) files.push(input.files![i])
    return files
  }
  return []
}

interface DropZoneProps {
  onUpload: (files: File[]) => void
}

export default function DropZone({ onUpload }: DropZoneProps) {
  const onDrop = useCallback(
    (accepted: File[]) => {
      if (accepted.length > 0) onUpload(accepted)
    },
    [onUpload],
  )

  const { getRootProps, getInputProps, isDragActive } = useDropzone({
    onDrop,
    getFilesFromEvent,
    accept: {
      'image/png': ['.png'],
      'image/jpeg': ['.jpg', '.jpeg'],
      'image/gif': ['.gif'],
      'image/webp': ['.webp'],
      'image/svg+xml': ['.svg'],
      'image/avif': ['.avif'],
      'image/bmp': ['.bmp'],
    },
    maxSize: 52_428_800,
    multiple: true,
  })

  return (
    <div
      {...getRootProps()}
      className={`cursor-pointer rounded-xl border-2 border-dashed p-12 text-center transition-colors ${
        isDragActive
          ? 'border-[var(--color-accent)] bg-[var(--color-accent-subtle)]'
          : 'border-[var(--color-border)] bg-[var(--glass-bg)] hover:border-[var(--color-border-hover)]'
      }`}
    >
      <input {...getInputProps()} />
      <div className="flex flex-col items-center gap-2 text-gray-400">
        <Upload className="h-8 w-8" />
        <p className="text-sm">
          {isDragActive
            ? 'Drop images here'
            : 'Drag & drop images, or click to select'}
        </p>
        <p className="text-xs text-gray-600">
          PNG, JPEG, GIF, WebP, SVG, AVIF, BMP — up to 50 MB each
        </p>
      </div>
    </div>
  )
}
