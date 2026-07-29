import { Link } from 'react-router-dom'
import { Settings } from 'lucide-react'
import { usePreprocessingStore } from '../stores/preprocessing'

function formatDimensions(w: number, h: number): string {
  return `${w}×${h}`
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
