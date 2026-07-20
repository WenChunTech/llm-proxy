import { normalizeProviderKind } from '../config/providers'
import type { ApiModel, ModelsPayload } from '../types/domain'

export function apiAuthHeaders(apiKey: string, headers?: HeadersInit) {
  const next = new Headers(headers)
  const key = apiKey.trim()
  if (key) next.set('authorization', `Bearer ${key}`)
  return next
}

export async function fetchModelCatalog(apiKey: string): Promise<ApiModel[]> {
  try {
    const response = await fetch('/api/models', {
      headers: apiAuthHeaders(apiKey),
      signal: AbortSignal.timeout(5000),
    })
    if (!response.ok) return []
    const payload = (await response.json()) as ModelsPayload
    return payload.data.filter((item) => item.id && normalizeProviderKind(item.owned_by))
  } catch {
    return []
  }
}
