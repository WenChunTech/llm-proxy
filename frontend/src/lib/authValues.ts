import { isRecord } from './records'

export function firstAuthBaseUrl(value: unknown): string {
  const items = Array.isArray(value) ? value : [value]
  for (const item of items) {
    if (isRecord(item) && typeof item.base_url === 'string' && item.base_url.trim()) {
      return item.base_url.trim()
    }
  }
  return ''
}

export function stringifyAuth(auth: unknown) {
  return auth ? JSON.stringify(auth, null, 2) : ''
}
