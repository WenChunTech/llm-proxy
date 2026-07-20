import type { ThemeMode } from '../types/domain'

export function readStoredThemeMode(): ThemeMode {
  const value = window.localStorage.getItem('llm-proxy-theme')
  return value === 'light' || value === 'dark' || value === 'system' ? value : 'system'
}

export function readStoredAccessKey() {
  return window.localStorage.getItem('llm-proxy-api-key') ?? ''
}

export function persistAccessKey(value: string) {
  const key = value.trim()
  if (key) {
    window.localStorage.setItem('llm-proxy-api-key', key)
    return
  }
  window.localStorage.removeItem('llm-proxy-api-key')
}
