import { useCallback } from 'react'
import { useDropzone } from 'react-dropzone'
import { Upload, Loader2 } from 'lucide-react'

interface DropZoneProps {
  onUpload: (file: File) => void
  isUploading: boolean
}

export default function DropZone({ onUpload, isUploading }: DropZoneProps) {
  const onDrop = useCallback(
    (accepted: File[]) => {
      if (accepted.length > 0) onUpload(accepted[0])
    },
    [onUpload],
  )

  const { getRootProps, getInputProps, isDragActive } = useDropzone({
    onDrop,
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
    disabled: isUploading,
    multiple: false,
  })

  return (
    <div
      {...getRootProps()}
      className={`cursor-pointer rounded-xl border-2 border-dashed p-12 text-center transition-colors ${
        isDragActive
          ? 'border-blue-500 bg-blue-500/10'
          : 'border-gray-700 bg-gray-900/50 hover:border-gray-500'
      } ${isUploading ? 'pointer-events-none opacity-50' : ''}`}
    >
      <input {...getInputProps()} />
      <div className="flex flex-col items-center gap-2 text-gray-400">
        {isUploading ? (
          <Loader2 className="h-8 w-8 animate-spin text-blue-400" />
        ) : (
          <Upload className="h-8 w-8" />
        )}
        <p className="text-sm">
          {isUploading
            ? 'Uploading…'
            : isDragActive
              ? 'Drop image here'
              : 'Drag & drop an image, or click to select'}
        </p>
        <p className="text-xs text-gray-600">
          PNG, JPEG, GIF, WebP, SVG, AVIF, BMP — up to 50 MB
        </p>
      </div>
    </div>
  )
}
