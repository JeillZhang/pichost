/**
 * Helpers for driving the custom GlassSelect component (replaces native <select>).
 *
 * GlassSelect renders a `button[role="combobox"]` trigger and, when opened,
 * a portal-rendered `div[role="listbox"]` containing `button[role="option"]` items.
 * Native <select> Playwright APIs (selectOption / toHaveValue) do not apply, so
 * tests open the trigger and click the target option by label.
 */
import { type Page, type Locator, expect } from '@playwright/test'

/** Open a GlassSelect and choose the option whose visible text matches `label`. */
export async function selectGlassOption(combo: Locator, label: string): Promise<void> {
  await combo.click()
  const option = combo.page().getByRole('option', { name: new RegExp(`^${escapeRegex(label)}$`) })
  await option.click()
}

/** Assert a GlassSelect currently displays `label` on its trigger. */
export async function expectGlassValue(combo: Locator, label: string): Promise<void> {
  await expect(combo).toHaveText(new RegExp(`^${escapeRegex(label)}`))
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

export type { Page }
