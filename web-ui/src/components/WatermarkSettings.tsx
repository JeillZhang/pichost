import { useState, useEffect, useCallback } from 'react'
import { toast } from 'sonner'
import { Loader2, Save, Trash2, Image } from 'lucide-react'
import { updateUserMe } from '../api/client'
import type { WatermarkConfig, UserProfile } from '../api/client'

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
      toast.success('Watermark settings saved')
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Failed to save watermark settings')
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
      toast.success('Watermark settings cleared')
    } catch (e: unknown) {
      toast.error(e instanceof Error ? e.message : 'Failed to clear watermark settings')
    } finally {
      setClearing(false)
    }
  }

  return (
    <div className="rounded-lg border border-[var(--color-border)] bg-[var(--glass-bg)] p-4 backdrop-blur-sm">
      <div className="mb-3 flex items-center gap-2">
        <Image className="h-4 w-4" style={{ color: 'var(--color-text-muted)' }} />
        <h3 className="text-sm font-medium" style={{ color: 'var(--color-text-primary)' }}>
          Default Watermark
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
        Enable watermark
      </label>

      {/* Config fields */}
      <fieldset disabled={!config.enabled} className="mt-3 space-y-3">
        {/* Text */}
        <div>
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Watermark Text</label>
          <input
            type="text"
            value={config.text}
            onChange={e => updateField('text', e.target.value)}
            placeholder="© Your Name"
            className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm disabled:opacity-50"
            style={{ color: 'var(--color-text-primary)' }}
          />
        </div>

        {/* Font */}
        <div>
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Font</label>
          <select
            value={config.font}
            onChange={e => updateField('font', e.target.value)}
            className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm disabled:opacity-50"
            style={{ color: 'var(--color-text-primary)' }}
          >
            {FONT_OPTIONS.map(f => (
              <option key={f} value={f}>{f}</option>
            ))}
          </select>
        </div>

        {/* Font size */}
        <div>
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Font Size</label>
          <input
            type="number"
            min={8}
            max={200}
            value={config.font_size}
            onChange={e => updateField('font_size', Number(e.target.value))}
            className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm disabled:opacity-50"
            style={{ color: 'var(--color-text-primary)' }}
          />
        </div>

        {/* Color */}
        <div>
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Color</label>
          <input
            type="text"
            value={config.color}
            onChange={e => updateField('color', e.target.value)}
            placeholder="rgba(255, 255, 255, 0.5)"
            className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm disabled:opacity-50"
            style={{ color: 'var(--color-text-primary)' }}
          />
        </div>

        {/* Rotation */}
        <div>
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>
            Rotation ({config.rotation}°)
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
            Scale ({config.scale.toFixed(2)})
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
          <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Position</label>
          <select
            value={config.position}
            onChange={e => updateField('position', e.target.value as WatermarkConfig['position'])}
            className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm disabled:opacity-50"
            style={{ color: 'var(--color-text-primary)' }}
          >
            {POSITION_OPTIONS.map(p => (
              <option key={p} value={p}>{p}</option>
            ))}
          </select>
        </div>

        {/* Margin X/Y */}
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Margin X</label>
            <input
              type="number"
              min={0}
              value={config.margin_x}
              onChange={e => updateField('margin_x', Number(e.target.value))}
              className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm disabled:opacity-50"
              style={{ color: 'var(--color-text-primary)' }}
            />
          </div>
          <div>
            <label className="block text-xs font-medium" style={{ color: 'var(--color-text-secondary)' }}>Margin Y</label>
            <input
              type="number"
              min={0}
              value={config.margin_y}
              onChange={e => updateField('margin_y', Number(e.target.value))}
              className="mt-1 block w-full rounded-lg border border-[var(--color-border)] bg-[var(--color-surface)] px-3 py-1.5 text-sm disabled:opacity-50"
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
          Save
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
          Clear
        </button>
      </div>
    </div>
  )
}
