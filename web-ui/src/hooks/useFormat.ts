import { useTranslation } from 'react-i18next'
import { formatBytes, formatDate, formatNumber } from '../lib/format'
export function useFormat() {
  const { i18n } = useTranslation()
  const locale = i18n.language
  return {
    formatBytes: (b: number) => formatBytes(b, locale),
    formatDate: (t: number) => formatDate(t, locale),
    formatNumber: (n: number) => formatNumber(n, locale),
  }
}
