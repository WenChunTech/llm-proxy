import { isRecord } from './records'

function authItems(value: unknown) {
  return Array.isArray(value) ? value : [value]
}

export function firstAuthBaseUrl(value: unknown): string {
  for (const item of authItems(value)) {
    if (isRecord(item) && typeof item.base_url === 'string' && item.base_url.trim()) {
      return item.base_url.trim()
    }
  }
  return ''
}

export function firstAuthAccessToken(value: unknown): string {
  for (const item of authItems(value)) {
    if (
      isRecord(item) &&
      item.disabled !== true &&
      typeof item.access_token === 'string' &&
      item.access_token.trim()
    ) {
      return item.access_token.trim()
    }
  }
  return ''
}

export function stringifyAuth(auth: unknown) {
  return auth ? JSON.stringify(auth, null, 2) : ''
}
