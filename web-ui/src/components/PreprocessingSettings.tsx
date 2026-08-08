import { useTranslation } from 'react-i18next'
import { usePreprocessingStore } from '../stores/preprocessing'
import GlassSelect from './ui/GlassSelect'

const FORMAT_OPTIONS = [
  { value: 'image/webp', label: 'WebP' },
  { value: 'image/jpeg', label: 'JPEG' },
  { value: 'image/png', label: 'PNG' },
] as const

const ROTATION_OPTIONS = [0, 90, 180, 270] as const

export function PreprocessingSettings() {
  const { t } = useTranslation()
  const store = usePreprocessingStore()

  return (
    <div className="space-y-4">
      <h3 className="text-lg font-semibold">{t('preprocessing.title')}</h3>
      <p className="text-sm" style={{ color: 'var(--color-text-muted)' }}>
        {t('preprocessing.intro')}
      </p>

      {/* EXIF Removal */}
      <div className="flex items-center justify-between">
        <div>
          <p className="font-medium">{t('preprocessing.removeExif')}</p>
          <p className="text-xs" style={{ color: 'var(--color-text-muted)' }}>
            {t('preprocessing.stripLocation')}
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
          <p className="font-medium">{t('preprocessing.resize')}</p>
          <input
            type="checkbox"
            checked={store.resize.enabled}
            onChange={(e) => store.updateResize({ ...store.resize, enabled: e.target.checked })}
            className="toggle"
          />
        </div>
        {store.resize.enabled && (
          <div className="flex items-center gap-3 pl-2">
            <label className="text-sm" style={{ color: 'var(--color-text-muted)' }}>{t('preprocessing.max')}</label>
            <input
              type="number"
              value={store.resize.maxWidth}
              onChange={(e) => store.updateResize({ ...store.resize, maxWidth: Number(e.target.value) || 1920 })}
              className="glass-input w-20 text-sm"
              min={1}
              max={10000}
            />
            <span style={{ color: 'var(--color-text-muted)' }}>×</span>
            <input
              type="number"
              value={store.resize.maxHeight}
              onChange={(e) => store.updateResize({ ...store.resize, maxHeight: Number(e.target.value) || 1920 })}
              className="glass-input w-20 text-sm"
              min={1}
              max={10000}
            />
            <span className="text-xs" style={{ color: 'var(--color-text-muted)' }}>{t('preprocessing.pxHint')}</span>
          </div>
        )}
      </div>

      {/* Format Convert */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <p className="font-medium">{t('preprocessing.convertFormat')}</p>
          <input
            type="checkbox"
            checked={store.formatConvert.enabled}
            onChange={(e) => store.updateFormatConvert({ ...store.formatConvert, enabled: e.target.checked })}
            className="toggle"
          />
        </div>
        {store.formatConvert.enabled && (
          <div className="flex items-center gap-3 pl-2">
            <GlassSelect
              value={store.formatConvert.targetFormat}
              onChange={(v) => store.updateFormatConvert({
                ...store.formatConvert,
                targetFormat: v as 'image/jpeg' | 'image/png' | 'image/webp',
              })}
              options={FORMAT_OPTIONS}
              className="w-40"
            />
            <label className="text-sm" style={{ color: 'var(--color-text-muted)' }}>{t('preprocessing.quality')}</label>
            <input
              type="range"
              min={10}
              max={100}
              value={store.formatConvert.quality}
              onChange={(e) => store.updateFormatConvert({ ...store.formatConvert, quality: Number(e.target.value) })}
              className="w-32"
            />
            <span className="text-sm tabular-nums w-8">{store.formatConvert.quality}</span>
          </div>
        )}
      </div>

      {/* Compression */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <p className="font-medium">{t('preprocessing.compression')}</p>
          <input
            type="checkbox"
            checked={store.compression.enabled}
            onChange={(e) => store.updateCompression({ ...store.compression, enabled: e.target.checked })}
            className="toggle"
          />
        </div>
        {store.compression.enabled && (
          <div className="flex items-center gap-3 pl-2">
            <label className="text-sm" style={{ color: 'var(--color-text-muted)' }}>{t('preprocessing.quality')}</label>
            <input
              type="range"
              min={10}
              max={100}
              value={store.compression.quality}
              onChange={(e) => store.updateCompression({ ...store.compression, quality: Number(e.target.value) })}
              className="w-32"
            />
            <span className="text-sm tabular-nums w-8">{store.compression.quality}</span>
          </div>
        )}
      </div>

      {/* Rotation */}
      <div className="space-y-2">
        <div className="flex items-center justify-between">
          <p className="font-medium">{t('preprocessing.rotation')}</p>
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
                type="button"
                onClick={() => store.updateRotate({ ...store.rotate, degrees: deg as 0 | 90 | 180 | 270 })}
                className={`px-3 py-1 rounded text-sm border transition-colors ${
                  store.rotate.degrees === deg
                    ? 'border-[var(--color-accent)] bg-[var(--color-accent-subtle)]'
                    : 'border-[var(--color-border)] hover:border-[var(--color-accent-subtle)]'
                }`}
                style={store.rotate.degrees === deg ? { color: 'var(--color-accent)' } : undefined}
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
          type="button"
          onClick={store.resetAll}
          className="text-sm transition-colors"
          style={{ color: 'var(--color-danger)' }}
          onMouseEnter={(e) => (e.currentTarget.style.color = 'var(--color-danger-hover)')}
          onMouseLeave={(e) => (e.currentTarget.style.color = 'var(--color-danger)')}
        >
          {t('preprocessing.reset')}
        </button>
      </div>
    </div>
  )
}
