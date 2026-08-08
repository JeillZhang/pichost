import { useState, useEffect, useCallback } from 'react'
import { useTranslation } from 'react-i18next'
import { toast } from 'sonner'
import { Loader2, Save, Trash2, Image } from 'lucide-react'
import { updateUserMe } from '../api/client'
import type { WatermarkConfig, UserProfile } from '../api/client'
import GlassSelect from './ui/GlassSelect'

const FONT_OPTIONS = [
  'NotoSansSC-Regular',
  'NotoSans-Regular',
  'Arial',
  'DejaVuSans',
  'FiraCode-Regular',
]

const POSITION_OPTIONS = [
  'top-left',
  'top-right',
  'bottom-left',
  'bottom-right',
  'center',
  'tile',
] as const

const DEFAULT_CONFIG: WatermarkConfig = {
  enabled: false,
  text: '',
  font: 'NotoSansSC-Regular',
  font_size: 48,
  color: 'rgba(255, 255, 255, 0.5)',
  rotation: -30,
  scale: 0.15,
  position: 'bottom-right',
  margin_x: 20,
  margin_y: 20,
}

interface WatermarkSettingsProps {
  profile: UserProfile | null
  onUpdate: (updated: UserProfile) => void
}

export default function WatermarkSettings({ profile, onUpdate }: WatermarkSettingsProps) {
  const { t } = useTranslation()
  const [config, setConfig] = useState<WatermarkConfig>(DEFAULT_CONFIG)
  const [saving, setSaving] = useState(false)
  const [clearing, setClearing] = useState(false)

  useEffect(() => {
    if (profile?.watermark_config) {
      setConfig(profile.watermark_config)
    } else {
      setConfig(DEFAULT_CONFIG)
    }
  }, [profile])

  const updateField = useCallback(<K extends keyof WatermarkConfig>(
    key: K,
    value: WatermarkConfig[K],
  ) => {
    setConfig(prev => ({ ...prev, [key]: value }))
  }, [])

  async function handleSave() {
    setSaving(true)
    try {
      const updated = await updateUserMe({ watermark_config: config })
      onUpdate(updated)
      toast.success(t('watermark.saved'))
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('watermark.saveFailed'))
    } finally {
      setSaving(false)
    }
  }

  async function handleClear() {
    setClearing(true)
    try {
      const updated = await updateUserMe({ watermark_config: null })
      setConfig(DEFAULT_CONFIG)
      onUpdate(updated)
      toast.success(t('watermark.cleared'))
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : t('watermark.clearFailed'))
    } finally {
      setClearing(false)
    }
  }

  return (
    <div className="rounded-lg border border-[var(--color-border)] bg-[var(--glass-tint-base)]/65 p-4 backdrop-blur-sm">
      <div className="mb-3 flex items-center gap-2">
        <Image className="h-4 w-4" style={{ color: 'var(--color-text-muted)' }} />
        <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
          {t('watermark.defaultWatermark')}
        </h3>
      </div>

      {/* Enable toggle */}
      <label className="flex items-center gap-2 text-sm" style={{ color: 'var(--color-text-secondary)' }}>
        <input
          type="checkbox"
          checked={config.enabled}
          onChange={e => updateField('enabled', e.target.checked)}
          className="rounded border-[var(--color-border)]"
          style={{ accentColor: 'var(--color-accent)' }}
        />
        {t('watermark.enable')}
      </label>

      {/* Config fields */}
      <fieldset disabled={!config.enabled} className="mt-3 space-y-3">
        {/* Text */}
        <div>
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>{t('watermark.text')}</label>
          <input
            type="text"
            value={config.text}
            onChange={e => updateField('text', e.target.value)}
            placeholder={t('watermark.textPlaceholder')}
            className="mt-1 block w-full rounded-lg border border-[var(--glass-border-base)] bg-[var(--glass-tint-base)]/65 px-3 py-1.5 text-sm disabled:opacity-50"
            style={{ color: 'var(--color-text-primary)' }}
          />
        </div>

        {/* Font */}
        <div>
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>{t('watermark.font')}</label>
          <GlassSelect
            value={config.font}
            onChange={(f) => updateField('font', f)}
            options={FONT_OPTIONS.map((f) => ({ value: f, label: f }))}
            className="mt-1"
          />
        </div>

        {/* Font size */}
        <div>
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>{t('watermark.fontSize')}</label>
          <input
            type="number"
            min={8}
            max={200}
            value={config.font_size}
            onChange={e => updateField('font_size', Number(e.target.value))}
            className="mt-1 block w-full rounded-lg border border-[var(--glass-border-base)] bg-[var(--glass-tint-base)]/65 px-3 py-1.5 text-sm disabled:opacity-50"
            style={{ color: 'var(--color-text-primary)' }}
          />
        </div>

        {/* Color */}
        <div>
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>{t('watermark.color')}</label>
          <input
            type="text"
            value={config.color}
            onChange={e => updateField('color', e.target.value)}
            placeholder="rgba(255, 255, 255, 0.5)"
            className="mt-1 block w-full rounded-lg border border-[var(--glass-border-base)] bg-[var(--glass-tint-base)]/65 px-3 py-1.5 text-sm disabled:opacity-50"
            style={{ color: 'var(--color-text-primary)' }}
          />
        </div>

        {/* Rotation */}
        <div>
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>
            {t('watermark.rotation', { value: config.rotation })}
          </label>
          <input
            type="range"
            min={-90}
            max={90}
            value={config.rotation}
            onChange={e => updateField('rotation', Number(e.target.value))}
            className="mt-1 w-full disabled:opacity-50"
            style={{ accentColor: 'var(--color-accent)' }}
          />
        </div>

        {/* Scale */}
        <div>
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>
            {t('watermark.scale', { value: config.scale.toFixed(2) })}
          </label>
          <input
            type="range"
            min={0.05}
            max={1}
            step={0.05}
            value={config.scale}
            onChange={e => updateField('scale', Number(e.target.value))}
            className="mt-1 w-full disabled:opacity-50"
            style={{ accentColor: 'var(--color-accent)' }}
          />
        </div>

        {/* Position */}
        <div>
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>{t('watermark.position')}</label>
          <GlassSelect
            value={config.position}
            onChange={(p) => updateField('position', p as WatermarkConfig['position'])}
            options={POSITION_OPTIONS.map((p) => ({ value: p, label: p }))}
            className="mt-1"
          />
        </div>

        {/* Margin X/Y */}
        <div className="grid grid-cols-1 gap-3 sm:grid-cols-2">
          <div>
            <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>{t('watermark.marginX')}</label>
            <input
              type="number"
              min={0}
              value={config.margin_x}
              onChange={e => updateField('margin_x', Number(e.target.value))}
              className="mt-1 block w-full rounded-lg border border-[var(--glass-border-base)] bg-[var(--glass-tint-base)]/65 px-3 py-1.5 text-sm disabled:opacity-50"
              style={{ color: 'var(--color-text-primary)' }}
            />
          </div>
          <div>
            <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>{t('watermark.marginY')}</label>
            <input
              type="number"
              min={0}
              value={config.margin_y}
              onChange={e => updateField('margin_y', Number(e.target.value))}
              className="mt-1 block w-full rounded-lg border border-[var(--glass-border-base)] bg-[var(--glass-tint-base)]/65 px-3 py-1.5 text-sm disabled:opacity-50"
              style={{ color: 'var(--color-text-primary)' }}
            />
          </div>
        </div>
      </fieldset>

      {/* Action buttons */}
      <div className="mt-4 flex gap-2">
        <button
          type="button"
          onClick={handleSave}
          disabled={saving || clearing}
          className="flex items-center gap-2 rounded-lg px-4 py-1.5 text-xs font-medium text-white disabled:opacity-50"
          style={{ backgroundColor: 'var(--color-accent)' }}
        >
          {saving ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Save className="h-3.5 w-3.5" />}
          {t('watermark.save')}
        </button>
        <button
          type="button"
          onClick={handleClear}
          disabled={clearing || saving}
          className="flex items-center gap-2 rounded-lg px-4 py-1.5 text-xs font-medium disabled:opacity-50"
          style={{
            backgroundColor: 'transparent',
            color: 'var(--color-danger)',
            border: '1px solid var(--color-border)',
          }}
        >
          {clearing ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Trash2 className="h-3.5 w-3.5" />}
          {t('watermark.clear')}
        </button>
      </div>
    </div>
  )
}
