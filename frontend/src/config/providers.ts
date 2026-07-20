import type { ProviderDraft, ProviderKind } from '../types/domain'
import { firstAuthBaseUrl } from '../lib/authValues'

export const providerMeta: Record<
  ProviderKind,
  { label: string; description: string; color: string; protocol: string }
> = {
  openai_chat: {
    label: 'OpenAI Chat',
    description: 'Chat Completions compatible endpoint',
    color: '#3b82f6',
    protocol: '/v1/chat/completions',
  },
  openai_responses: {
    label: 'OpenAI Responses',
    description: 'Responses API and reasoning models',
    color: '#16a085',
    protocol: '/v1/responses',
  },
  claude: {
    label: 'Claude',
    description: 'Anthropic Messages compatible endpoint',
    color: '#e07a5f',
    protocol: '/v1/messages',
  },
  gemini: {
    label: 'Gemini',
    description: 'Google Generative Language endpoint',
    color: '#7c6cf2',
    protocol: '/v1beta/models/{model}',
  },
  codex: {
    label: 'Codex',
    description: 'Codex Responses provider',
    color: '#f59e0b',
    protocol: '/v1/responses',
  },
  grok: {
    label: 'Grok',
    description: 'xAI Responses provider',
    color: '#6ee7f9',
    protocol: '/v1/responses',
  },
}

export const defaultPriority: ProviderKind[] = [
  'openai_responses',
  'openai_chat',
  'claude',
  'gemini',
  'grok',
  'codex',
]

export const editableNewKinds: ProviderKind[] = defaultPriority

export function normalizePriority(value: ProviderKind[]) {
  const seen = new Set<ProviderKind>()
  const next: ProviderKind[] = []
  for (const item of value.length ? value : defaultPriority) {
    if (providerMeta[item] && !seen.has(item)) {
      seen.add(item)
      next.push(item)
    }
  }
  for (const item of defaultPriority) {
    if (!seen.has(item)) next.push(item)
  }
  return next
}

export function defaultBaseUrlFor(kind: ProviderKind) {
  if (kind === 'codex') return 'https://chatgpt.com/backend-api/codex'
  if (kind === 'grok') return 'https://api.x.ai/v1'
  return 'https://'
}

export function defaultBaseUrlForNewProvider(kind: ProviderKind) {
  return kind === 'codex' || kind === 'grok' ? '' : defaultBaseUrlFor(kind)
}

export function defaultBaseUrlForImportedProvider(kind: ProviderKind) {
  return kind === 'codex' || kind === 'grok' ? '' : defaultBaseUrlFor(kind)
}

export function effectiveBaseUrlForProvider(provider: Pick<ProviderDraft, 'kind' | 'baseUrl' | 'auth'>) {
  return effectiveBaseUrlFor(provider.kind, provider.baseUrl, provider.auth)
}

export function effectiveBaseUrlFor(kind: ProviderKind, baseUrl: string, auth: unknown) {
  const customBaseUrl = baseUrl.trim()
  if (customBaseUrl) return customBaseUrl
  if (kind === 'codex' || kind === 'grok') {
    return firstAuthBaseUrl(auth) || defaultBaseUrlFor(kind)
  }
  return customBaseUrl
}

export function providerMarkText(kind: ProviderKind) {
  if (kind === 'grok') return 'xAI'
  return providerMeta[kind].label.slice(0, 1)
}

export function normalizeProviderKind(value: string): ProviderKind | null {
  if (value === 'xai') return 'grok'
  return providerMeta[value as ProviderKind] ? (value as ProviderKind) : null
}
