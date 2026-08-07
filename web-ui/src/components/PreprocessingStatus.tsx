import { Link } from 'react-router-dom'
import { Settings } from 'lucide-react'
import { useTranslation } from 'react-i18next'
import { usePreprocessingStore } from '../stores/preprocessing'

function formatDimensions(w: number, h: number): string {
  return `${w}×${h}`
}

export function PreprocessingStatus() {
  const { t } = useTranslation()
  const prefs = usePreprocessingStore()
  const hasAny = prefs.hasAnyEnabled()

  if (!hasAny) {
    return (
      <div className="flex items-center gap-2 text-xs" style={{ color: 'var(--color-text-muted)' }}>
        <span>{t('preprocessing.off')}</span>
        <Link
          to="/settings"
          className="underline underline-offset-2"
          style={{ color: 'var(--color-accent)' }}
        >
          {t('preprocessing.configure')}
        </Link>
      </div>
    )
  }

  const tags: string[] = []
  if (prefs.stripExif) tags.push(t('preprocessing.exifOn'))
  if (prefs.resize.enabled) {
    tags.push(t('preprocessing.resizeLabel', {
      dimensions: formatDimensions(prefs.resize.maxWidth, prefs.resize.maxHeight),
    }))
  }
  if (prefs.formatConvert.enabled) {
    const fmt = prefs.formatConvert.targetFormat.split('/')[1].toUpperCase()
    tags.push(t('preprocessing.formatLabel', { format: fmt, quality: prefs.formatConvert.quality }))
  }
  if (prefs.compression.enabled) {
    tags.push(t('preprocessing.compressLabel', { quality: prefs.compression.quality }))
  }
  if (prefs.rotate.enabled) {
    tags.push(t('preprocessing.rotateLabel', { degrees: prefs.rotate.degrees }))
  }

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
        {t('preprocessing.configure')}
      </Link>
    </div>
  )
}
