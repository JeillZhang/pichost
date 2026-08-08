import i18n from 'i18next'
import { initReactI18next } from 'react-i18next'
import LanguageDetector from 'i18next-browser-languagedetector'
import en from './locales/en.json'
import zhCN from './locales/zh-CN.json'

export function getCurrentLocale(): string {
  return i18n.language.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en'
}
export function applyLang(lng: string): void {
  document.documentElement.lang = lng.toLowerCase().startsWith('zh') ? 'zh-CN' : 'en'
}

i18n.use(LanguageDetector)
    .use(initReactI18next)
    .init({
      resources: { en: { translation: en }, 'zh-CN': { translation: zhCN } },
      supportedLngs: ['en', 'zh-CN'],
      fallbackLng: 'en',
      detection: {
        order: ['localStorage', 'navigator'],
        caches: ['localStorage'],
        lookupLocalStorage: 'pichost-locale',
      },
      interpolation: { escapeValue: false },
      ns: ['translation'],
    })

applyLang(i18n.language)
i18n.on('languageChanged', applyLang)

export default i18n
