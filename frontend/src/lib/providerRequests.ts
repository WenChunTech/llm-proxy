import { effectiveBaseUrlForProvider } from '../config/providers'
import type { ProviderDraft, ProviderKind, ProviderModelEntry } from '../types/domain'
import { shellQuote } from './browser'

export function buildProviderTestCurl(provider: ProviderDraft, model: string, prompt: string, stream: boolean) {
  const { endpoint, headers, body } = buildProviderTestRequest(provider, model, prompt, stream)
  const headerArgs = Object.entries(headers)
    .map(([name, value]) => `-H ${shellQuote(`${name}: ${value}`)}`)
    .join(' ')
  return `curl -sS -X POST ${shellQuote(endpoint)} ${headerArgs} --data-raw ${shellQuote(JSON.stringify(body))}`
}

function buildProviderTestRequest(provider: ProviderDraft, model: string, prompt: string, stream: boolean) {
  const endpoint = getProviderTestEndpoint(
    provider.kind,
    effectiveBaseUrlForProvider(provider),
    model,
    stream,
  )
  const headers: Record<string, string> = { 'content-type': 'application/json' }
  if (stream) headers.accept = 'text/event-stream'
  if (provider.kind === 'claude') {
    headers['x-api-key'] = provider.apiKey
    headers['anthropic-version'] = '2023-06-01'
  } else if (provider.kind === 'gemini') {
    headers['x-goog-api-key'] = provider.apiKey
    headers.authorization = `Bearer ${provider.apiKey}`
  } else {
    headers.authorization = `Bearer ${provider.apiKey}`
  }
  if (provider.kind === 'codex') {
    headers['user-agent'] = 'codex-tui/0.135.0'
    headers.originator = 'codex-tui'
  }

  return {
    endpoint,
    headers,
    body: buildProviderTestBody(provider.kind, model, prompt, stream),
  }
}

function buildProviderTestBody(kind: ProviderKind, model: string, prompt: string, stream: boolean) {
  if (kind === 'openai_chat' || kind === 'claude') {
    return {
      model,
      messages: [{ role: 'user', content: prompt }],
      stream,
      max_tokens: 16,
    }
  }
  if (kind === 'gemini') {
    return {
      contents: [{
        role: 'user',
        parts: [{ text: prompt }],
      }],
    }
  }
  const body: Record<string, unknown> = {
    model,
    input: prompt,
    stream,
    max_output_tokens: 16,
  }
  if (kind === 'codex') {
    body.store = false
    body.instructions = ''
  }
  return body
}

export function hasUsableProviderConnection(provider: ProviderDraft) {
  return Boolean(getProviderModelsEndpoint(provider))
}

function getProviderModelsEndpoint(provider: ProviderDraft) {
  return getProviderModelsEndpointFor(provider.kind, effectiveBaseUrlForProvider(provider))
}

export function getProviderModelsEndpointFor(kind: ProviderKind, baseUrl: string) {
  try {
    const url = new URL(baseUrl.trim())
    if (!url.host || (url.protocol !== 'http:' && url.protocol !== 'https:')) return ''
    const pathname = url.pathname.replace(/\/+$/, '')
    if (pathname.endsWith('/models')) {
      url.pathname = pathname || '/models'
      return url.toString()
    }

    const isOpenAIStyle = ['openai_chat', 'openai_responses', 'codex', 'grok'].includes(kind)
    const hasVersionPath = /\/v\d+$/.test(pathname)
    const suffix = !isOpenAIStyle && !hasVersionPath ? 'v1/models' : 'models'
    url.pathname = appendUrlPath(pathname, suffix)
    return url.toString()
  } catch {
    return ''
  }
}

function getProviderTestEndpoint(kind: ProviderKind, baseUrl: string, model: string, stream: boolean) {
  const suffix =
    kind === 'openai_chat' ? 'chat/completions'
      : kind === 'openai_responses' || kind === 'codex' || kind === 'grok' ? 'responses'
        : kind === 'claude' ? 'v1/messages'
          : `v1beta/models/${encodeURIComponent(model)}:${stream ? 'streamGenerateContent?alt=sse' : 'generateContent'}`
  return getProviderEndpointFor(baseUrl, suffix)
}

function getProviderEndpointFor(baseUrl: string, suffix: string) {
  try {
    const url = new URL(baseUrl.trim())
    if (!url.host || (url.protocol !== 'http:' && url.protocol !== 'https:')) return baseUrl.trim()
    const pathname = url.pathname.replace(/\/+$/, '')
    const [suffixPathPart, suffixQuery = ''] = suffix.split('?')
    const suffixPath = suffixPathPart.replace(/^\/+/, '')
    const suffixSegments = suffixPath.split('/')
    const lastSegment = suffixSegments[suffixSegments.length - 1]
    if (lastSegment && pathname.endsWith(`/${lastSegment}`)) {
      url.pathname = pathname || `/${lastSegment}`
    } else {
      url.pathname = appendUrlPath(pathname, suffixPath)
    }
    if (suffixQuery) url.search = suffixQuery
    return url.toString()
  } catch {
    return baseUrl.trim()
  }
}

function appendUrlPath(basePath: string, path: string) {
  const base = basePath.replace(/\/+$/, '')
  const next = path.replace(/^\/+/, '')
  return base ? `${base}/${next}` : `/${next}`
}

export function normalizeProviderModelEntries(data: ProviderModelEntry[]) {
  return Array.from(
    new Set(
      data
        .map((item) => (typeof item === 'string' ? item : item.id))
        .filter((item): item is string => Boolean(item?.trim()))
        .map((item) => item.trim()),
    ),
  ).sort()
}
