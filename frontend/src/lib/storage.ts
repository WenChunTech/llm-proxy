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

const AUTH_VALIDATION_CONCURRENCY_KEY = 'llm-proxy-auth-validation-concurrency'
const DEFAULT_AUTH_VALIDATION_CONCURRENCY = 5
const MIN_AUTH_VALIDATION_CONCURRENCY = 1

export function normalizeAuthValidationConcurrency(value: unknown) {
  if (value === null || value === undefined || value === '') {
    return DEFAULT_AUTH_VALIDATION_CONCURRENCY
  }
  const numeric = typeof value === 'number' ? value : Number(value)
  if (!Number.isFinite(numeric)) return DEFAULT_AUTH_VALIDATION_CONCURRENCY
  return Math.max(MIN_AUTH_VALIDATION_CONCURRENCY, Math.floor(numeric))
}

export function readStoredAuthValidationConcurrency() {
  return normalizeAuthValidationConcurrency(
    window.localStorage.getItem(AUTH_VALIDATION_CONCURRENCY_KEY),
  )
}

export function persistAuthValidationConcurrency(value: number) {
  const next = normalizeAuthValidationConcurrency(value)
  window.localStorage.setItem(AUTH_VALIDATION_CONCURRENCY_KEY, String(next))
  return next
}
