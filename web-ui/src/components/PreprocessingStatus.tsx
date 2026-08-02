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
      <div className="flex items-center gap-2 text-xs" style={{ color: 'var(--color-text-muted)' }}>
        <span>Preprocessing: Off</span>
        <Link
          to="/settings"
          className="underline underline-offset-2"
          style={{ color: 'var(--color-accent)' }}
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
          className="rounded px-1.5 py-0.5 border text-xs"
          style={{
            backgroundColor: 'var(--color-accent-subtle)',
            color: 'var(--color-accent)',
            borderColor: 'var(--color-accent-subtle)',
          }}
        >
          {tag}
        </span>
      ))}
      <Link
        to="/settings"
        className="ml-1 underline underline-offset-2 flex items-center gap-1"
        style={{ color: 'var(--color-accent)' }}
      >
        <Settings className="h-3 w-3" />
        Configure...
      </Link>
    </div>
  )
}
