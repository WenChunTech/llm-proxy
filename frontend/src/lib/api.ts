import { normalizeProviderKind } from '../config/providers'
import type { ApiModel, ModelsPayload } from '../types/domain'

export function apiAuthHeaders(apiKey: string, headers?: HeadersInit) {
  const next = new Headers(headers)
  const key = apiKey.trim()
  if (key) next.set('authorization', `Bearer ${key}`)
  return next
}

const inflightModelCatalog = new Map<string, Promise<ApiModel[]>>()

/** Fetch model catalog with in-flight dedupe per API key. */
export async function fetchModelCatalog(apiKey: string): Promise<ApiModel[]> {
  const key = apiKey.trim()
  const existing = inflightModelCatalog.get(key)
  if (existing) return existing

  const request = (async () => {
    try {
      const response = await fetch('/api/models', {
        headers: apiAuthHeaders(key),
        signal: AbortSignal.timeout(5000),
      })
      if (!response.ok) return []
      const payload = (await response.json()) as ModelsPayload
      return payload.data.filter((item) => item.id && normalizeProviderKind(item.owned_by))
    } catch {
      return []
    } finally {
      inflightModelCatalog.delete(key)
    }
  })()

  inflightModelCatalog.set(key, request)
  return request
}

export function remoteModelsFrom(payloadModels: string[], catalog: ApiModel[]) {
  return Array.from(
    new Set([...payloadModels, ...catalog.map((item) => item.id)]),
  )
}
