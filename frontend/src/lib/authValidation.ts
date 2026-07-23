import type {
  AuthProviderKind,
  AuthValidationFilter,
  AuthValidationPayload,
  AuthValidationResult,
  AuthValidationState,
  AuthValidationTarget,
  Provider,
} from '../types/domain'
import { providerMeta } from '../config/providers'
import { isRecord } from './records'

export function buildAuthValidationConfig(providers: Provider[]) {
  return {
    codex: providers
      .filter((provider) => provider.kind === 'codex')
      .map(toAuthValidationProviderConfig),
    grok: providers
      .filter((provider) => provider.kind === 'grok')
      .map(toAuthValidationProviderConfig),
  }
}

function toAuthValidationProviderConfig(provider: Provider) {
  return {
    enabled: provider.enabled,
    base_url: provider.baseUrl,
    auth: provider.auth,
  }
}

export function applyAuthValidationResults(
  providers: Provider[],
  kind: AuthProviderKind,
  results: AuthValidationResult[],
) {
  const byProvider = new Map<number, AuthValidationResult[]>()
  for (const result of results) {
    if (result.skipped || result.auth === null || result.auth === undefined) continue
    const items = byProvider.get(result.providerIndex) ?? []
    items.push(result)
    byProvider.set(result.providerIndex, items)
  }
  let providerIndex = -1
  return providers.map((provider) => {
    if (provider.kind !== kind) return provider
    providerIndex += 1
    const providerResults = byProvider.get(providerIndex)
    if (!providerResults?.length) return provider
    const shouldEnableProvider = providerResults.some(shouldEnableAuthFromResult)
    const usesArray = providerResults.some((result) => result.isAuthArray) || Array.isArray(provider.auth)
    if (!usesArray) {
      return {
        ...provider,
        enabled: shouldEnableProvider ? true : provider.enabled,
        auth: normalizedAuthFromValidationResult(providerResults[0]),
      }
    }
    const currentItems = Array.isArray(provider.auth) ? [...provider.auth] : provider.auth ? [provider.auth] : []
    for (const result of providerResults) {
      currentItems[result.authIndex] = normalizedAuthFromValidationResult(result)
    }
    return {
      ...provider,
      enabled: shouldEnableProvider ? true : provider.enabled,
      auth: currentItems,
    }
  })
}

function normalizedAuthFromValidationResult(result: AuthValidationResult) {
  return shouldEnableAuthFromResult(result)
    ? setAuthValueDisabled(result.auth, 0, false)
    : result.auth
}

function shouldEnableAuthFromResult(result: AuthValidationResult) {
  return result.valid && !result.skipped && result.reason !== 'rate_limited'
}

export function mergeAuthValidationState(
  current: AuthValidationState | null,
  kind: AuthProviderKind,
  payload: AuthValidationPayload,
): AuthValidationState {
  if (!current || current.kind !== kind) {
    return {
      kind,
      payload: normalizeAuthValidationPayloadStats(payload),
      filter: 'all',
    }
  }
  const merged = new Map<string, AuthValidationResult>()
  for (const result of current.payload.results) {
    merged.set(authValidationResultKey(result), result)
  }
  for (const result of payload.results) {
    merged.set(authValidationResultKey(result), result)
  }
  return {
    ...current,
    payload: normalizeAuthValidationPayloadStats({
      ...payload,
      results: Array.from(merged.values()).sort(
        (a, b) => a.providerIndex - b.providerIndex || a.authIndex - b.authIndex,
      ),
    }),
  }
}

export function normalizeAuthValidationPayloadStats(payload: AuthValidationPayload): AuthValidationPayload {
  const results = payload.results
  return {
    ...payload,
    total: results.length,
    checked: results.filter((result) => !result.skipped).length,
    valid: results.filter((result) => result.valid && result.reason !== 'rate_limited').length,
    invalid: results.filter((result) => !result.valid && !result.skipped).length,
    skipped: results.filter((result) => result.skipped).length,
    rateLimited: results.filter((result) => result.reason === 'rate_limited').length,
    refreshed: results.filter((result) => result.refreshed).length,
  }
}

export function authValidationResultKey(result: AuthValidationTarget) {
  return `${result.providerIndex}:${result.authIndex}`
}

export function visibleAuthValidationResults(state: AuthValidationState) {
  return state.payload.results.filter((result) => matchesAuthValidationFilter(result, state.filter))
}

export function matchesAuthValidationFilter(result: AuthValidationResult, filter: AuthValidationFilter) {
  if (filter === 'all') return true
  if (filter === 'ok') return result.valid && !result.skipped && result.reason !== 'rate_limited'
  if (filter === 'invalid') return !result.valid && !result.skipped
  if (filter === 'rate_limited') return result.reason === 'rate_limited'
  return result.disabled
}

export function syncAuthValidationPayloadWithProviders(
  payload: AuthValidationPayload,
  providers: Provider[],
  kind: AuthProviderKind,
) {
  return normalizeAuthValidationPayloadStats({
    ...payload,
    results: payload.results
      .map((result) => syncAuthValidationResultWithProviders(result, providers, kind))
      .filter((result): result is AuthValidationResult => Boolean(result)),
  })
}

export function syncAuthValidationResultWithProviders(
  result: AuthValidationResult,
  providers: Provider[],
  kind: AuthProviderKind,
): AuthValidationResult | null {
  const target = findProviderAuthTarget(providers, kind, result)
  if (!target) return null
  return {
    ...result,
    authCount: target.authCount,
    isAuthArray: target.isAuthArray,
    disabled: !target.provider.enabled || authValueDisabled(target.auth),
    label: authValidationResultLabel(kind, result.providerIndex, result.authIndex, target.authCount, target.auth),
    auth: target.auth,
  }
}

export function setProviderAuthDisabled(
  providers: Provider[],
  kind: AuthProviderKind,
  target: AuthValidationTarget,
  disabled: boolean,
) {
  const providerArrayIndex = providerArrayIndexForKind(providers, kind, target.providerIndex)
  if (providerArrayIndex < 0) return providers
  const provider = providers[providerArrayIndex]
  const nextAuth = setAuthValueDisabled(provider.auth, target.authIndex, disabled)
  const nextEnabled = disabled ? provider.enabled : true
  if (nextAuth === provider.auth && nextEnabled === provider.enabled) return providers
  return providers.map((item, index) =>
    index === providerArrayIndex ? { ...item, enabled: nextEnabled, auth: nextAuth } : item,
  )
}

export function deleteProviderAuthTarget(
  providers: Provider[],
  kind: AuthProviderKind,
  target: AuthValidationTarget,
) {
  const providerArrayIndex = providerArrayIndexForKind(providers, kind, target.providerIndex)
  if (providerArrayIndex < 0) return providers
  const provider = providers[providerArrayIndex]
  const nextAuth = deleteAuthValueTarget(provider.auth, target.authIndex)
  if (nextAuth === provider.auth) return providers
  return providers.map((item, index) =>
    index === providerArrayIndex ? { ...item, auth: nextAuth } : item,
  )
}

export function providerArrayIndexForKind(providers: Provider[], kind: AuthProviderKind, providerIndex: number) {
  let seen = -1
  return providers.findIndex((provider) => {
    if (provider.kind !== kind) return false
    seen += 1
    return seen === providerIndex
  })
}

export function findProviderAuthTarget(
  providers: Provider[],
  kind: AuthProviderKind,
  target: AuthValidationTarget,
) {
  const providerArrayIndex = providerArrayIndexForKind(providers, kind, target.providerIndex)
  if (providerArrayIndex < 0) return null
  const provider = providers[providerArrayIndex]
  const authItems = Array.isArray(provider.auth)
    ? provider.auth
    : provider.auth === undefined || provider.auth === null ? [] : [provider.auth]
  const auth = authItems[target.authIndex]
  if (auth === undefined) return null
  return {
    provider,
    auth,
    authCount: authItems.length,
    isAuthArray: Array.isArray(provider.auth),
  }
}

export function setAuthValueDisabled(auth: unknown, authIndex: number, disabled: boolean): unknown {
  if (Array.isArray(auth)) {
    if (authIndex < 0 || authIndex >= auth.length || !isRecord(auth[authIndex])) return auth
    return auth.map((item, index) =>
      index === authIndex ? { ...(item as Record<string, unknown>), disabled } : item,
    )
  }
  if (authIndex !== 0 || !isRecord(auth)) return auth
  return { ...auth, disabled }
}

export function deleteAuthValueTarget(auth: unknown, authIndex: number): unknown {
  if (Array.isArray(auth)) {
    if (authIndex < 0 || authIndex >= auth.length) return auth
    return auth.filter((_, index) => index !== authIndex)
  }
  if (authIndex !== 0 || auth === undefined || auth === null) return auth
  return []
}

export function authValueDisabled(auth: unknown) {
  return isRecord(auth) && auth.disabled === true
}

export function authValidationResultLabel(
  kind: AuthProviderKind,
  providerIndex: number,
  authIndex: number,
  authCount: number,
  auth: unknown,
) {
  if (isRecord(auth) && typeof auth.email === 'string' && auth.email.trim()) {
    return auth.email.trim()
  }
  const providerLabel = providerMeta[kind].label
  return authCount > 1
    ? `${providerLabel} #${providerIndex + 1} auth #${authIndex + 1}`
    : `${providerLabel} #${providerIndex + 1}`
}

export function authValidationReasonLabel(reason: string) {
  const labels: Record<string, string> = {
    ok: '有效',
    invalid_auth: '认证无效',
    rate_limited: '限流',
    payment_required: '需付费',
    forbidden: '禁止访问',
    request_error: '请求受限',
    server_error: '服务端错误',
    network_error: '网络错误',
    refresh_failed: '刷新失败',
    missing_access_token: '缺少 token',
    no_auth: '无 auth',
    invalid_auth_json: 'JSON 无效',
  }
  return labels[reason] ?? reason
}
