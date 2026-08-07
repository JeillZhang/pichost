const UNITS = ['B', 'KB', 'MB', 'GB', 'TB']
export function formatBytes(bytes: number, locale: string): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  const i = Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), UNITS.length - 1)
  const value = bytes / 1024 ** i
  const digits = i === 0 ? 0 : 1
  return `${new Intl.NumberFormat(locale, { maximumFractionDigits: digits, useGrouping: false }).format(value)} ${UNITS[i]}`
}
export function formatDate(ts: number, locale: string): string {
  return new Intl.DateTimeFormat(locale, {
    month: 'short', day: 'numeric', hour: '2-digit', minute: '2-digit',
  }).format(new Date(ts))
}
export function formatNumber(n: number, locale: string): string {
  return new Intl.NumberFormat(locale).format(n)
}
