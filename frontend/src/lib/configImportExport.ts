import {
  defaultBaseUrlForImportedProvider,
  defaultPriority,
  providerMeta,
} from '../config/providers'
import type { ApiProvider, Provider, ProviderDraft, ProviderKind, RetryConfig } from '../types/domain'
import { isRecord } from './records'

export function fromApiProvider(provider: ApiProvider): Provider {
  return {
    id: provider.id,
    kind: provider.kind,
    name: provider.name,
    baseUrl: provider.base_url,
    apiKey: provider.api_key,
    models: provider.models,
    enabled: provider.enabled,
    auth: provider.auth,
  }
}

export function toApiProvider(provider: Provider): ApiProvider {
  return {
    id: provider.id,
    kind: provider.kind,
    name: provider.name,
    enabled: provider.enabled,
    base_url: provider.baseUrl,
    api_key: provider.apiKey,
    models: provider.models,
    auth: provider.auth,
  }
}

export function toApiProviderDraft(provider: ProviderDraft): ApiProvider {
  return {
    id: 'draft',
    kind: provider.kind,
    name: provider.name,
    enabled: provider.enabled,
    base_url: provider.baseUrl,
    api_key: provider.apiKey,
    models: provider.models,
    auth: provider.auth,
  }
}

export function dedupeProvidersForSave(providers: Provider[]) {
  const seen = new Set<string>()
  return providers.filter((provider) => {
    const key = providerDedupeKey(provider)
    if (!key) return true
    if (seen.has(key)) return false
    seen.add(key)
    return true
  })
}

function providerDedupeKey(provider: Provider) {
  if (provider.kind === 'codex' || provider.kind === 'grok') {
    const apiKey = provider.apiKey.trim()
    return apiKey ? `${provider.kind}:api_key:${apiKey}` : ''
  }

  const baseUrl = provider.baseUrl.trim()
  return baseUrl ? `${provider.kind}:base_url:${baseUrl}` : ''
}

export function buildConfigExport(
  providers: Provider[],
  priority: ProviderKind[],
  fallbacks: string[],
  retry: RetryConfig,
  apiKey: string,
  port: number,
) {
  return {
    port,
    api_key: apiKey,
    model_priority: priority,
    fallback_models: fallbacks,
    providers: Object.fromEntries(
      defaultPriority.map((kind) => [
        kind,
        providers
          .filter((provider) => provider.kind === kind)
          .map(toRuntimeProviderConfig),
      ]),
    ),
    retry: {
      max_retries: retry.maxRetries,
      backoff_step_ms: retry.backoffStepMs,
    },
  }
}

export function toRuntimeProviderConfig(provider: Provider) {
  return {
    enabled: provider.enabled,
    models: provider.models,
    base_url: provider.baseUrl,
    api_key: provider.apiKey,
    ...(provider.auth === undefined ? {} : { auth: provider.auth }),
  }
}

export function providersFromImport(value: unknown): Provider[] {
  if (!isRecord(value) || !isRecord(value.providers)) return []
  return importProvidersByKind(value.providers)
}

function importProvidersByKind(value: Record<string, unknown>) {
  return defaultPriority.flatMap((kind) => {
    const entries = value[kind]
    if (!Array.isArray(entries)) return []
    return entries
      .map((entry, index) => importConfigProvider(kind, entry, index))
      .filter((item): item is Provider => Boolean(item))
  })
}

function importConfigProvider(kind: ProviderKind, value: unknown, index: number): Provider | null {
  if (!isRecord(value)) return null
  const auth = value.auth
  return {
    id: `import:${kind}:${Date.now()}:${index}`,
    kind,
    name: `${providerMeta[kind].label} import`,
    enabled: typeof value.enabled === 'boolean' ? value.enabled : true,
    baseUrl: typeof value.base_url === 'string'
      ? value.base_url
      : defaultBaseUrlForImportedProvider(kind),
    apiKey: typeof value.api_key === 'string' ? value.api_key : '',
    models: normalizeStringArray(value.models),
    auth: kind === 'codex' || kind === 'grok' ? auth : undefined,
  }
}

function normalizeStringArray(value: unknown) {
  return Array.isArray(value)
    ? value.filter((item): item is string => typeof item === 'string' && Boolean(item.trim()))
    : []
}
