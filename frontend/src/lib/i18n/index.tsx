import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react'
import { translations, type Language, type TranslationKey } from './translations'

const STORAGE_KEY = 'llm-proxy-language'

function detectSystemLanguage(): Language {
  const lang = typeof navigator !== 'undefined' ? navigator.language : 'en'
  return lang.toLowerCase().startsWith('zh') ? 'zh' : 'en'
}

function readStoredLanguage(): Language {
  if (typeof window === 'undefined') return detectSystemLanguage()
  const value = window.localStorage.getItem(STORAGE_KEY)
  return value === 'en' || value === 'zh' ? value : detectSystemLanguage()
}

type Params = Record<string, string | number> | undefined

/** Module-level language state for use in non-React modules. */
let currentLanguage: Language = readStoredLanguage()
const listeners = new Set<() => void>()

function notifyListeners() {
  for (const listener of listeners) listener()
}

/** Module-level translate function for use outside React components. */
export function t(key: TranslationKey, params?: Params): string {
  const entry = translations[key]
  if (!entry) return key
  const template = entry[currentLanguage] ?? entry.en
  return interpolate(template, params)
}

export function getCurrentLanguage(): Language {
  return currentLanguage
}

export function setLanguageModule(language: Language) {
  if (language === currentLanguage) return
  currentLanguage = language
  if (typeof window !== 'undefined') {
    window.localStorage.setItem(STORAGE_KEY, language)
  }
  notifyListeners()
}

/** Subscribe to module-level language changes (for non-React modules). */
export function subscribeLanguageChange(listener: () => void): () => void {
  listeners.add(listener)
  return () => listeners.delete(listener)
}

function interpolate(template: string, params?: Params): string {
  if (!params) return template
  return template.replace(/\{(\w+)\}/g, (_, name: string) => {
    const value = params[name]
    return value !== undefined ? String(value) : `{${name}}`
  })
}

type I18nContextValue = {
  language: Language
  setLanguage: (language: Language) => void
  t: (key: TranslationKey, params?: Params) => string
}

const I18nContext = createContext<I18nContextValue | null>(null)

export function I18nProvider({ children }: { children: ReactNode }) {
  const [language, setLanguageState] = useState<Language>(currentLanguage)

  useEffect(() => {
    return subscribeLanguageChange(() => {
      setLanguageState(currentLanguage)
    })
  }, [])

  const setLanguage = useCallback((next: Language) => {
    setLanguageModule(next)
  }, [])

  const translate = useCallback(
    (key: TranslationKey, params?: Params) => {
      const entry = translations[key]
      if (!entry) return key
      return interpolate(entry[language] ?? entry.en, params)
    },
    [language],
  )

  const value = useMemo(
    () => ({ language, setLanguage, t: translate }),
    [language, setLanguage, translate],
  )

  return <I18nContext.Provider value={value}>{children}</I18nContext.Provider>
}

export function useI18n(): I18nContextValue {
  const ctx = useContext(I18nContext)
  if (!ctx) throw new Error('useI18n must be used within an I18nProvider')
  return ctx
}

export type { Language, TranslationKey }
