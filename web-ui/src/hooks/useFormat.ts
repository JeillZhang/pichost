import { useTranslation } from 'react-i18next'
import { getCurrentLocale } from '../i18n'
import { formatBytes, formatDate, formatNumber } from '../lib/format'
export function useFormat() {
  // Subscribes to language changes so formatting re-runs with the new locale.
  useTranslation()
  const locale = getCurrentLocale()
  return {
    formatBytes: (b: number) => formatBytes(b, locale),
    formatDate: (t: number) => formatDate(t, locale),
    formatNumber: (n: number) => formatNumber(n, locale),
  }
}
