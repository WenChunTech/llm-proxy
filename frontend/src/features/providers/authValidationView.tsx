import { Icon } from '../../components/Icon'
import {
  authValueDisabled,
  authValidationReasonLabel,
  matchesAuthValidationFilter,
} from '../../lib/authValidation'
import type {
  AuthProviderKind,
  AuthValidationFilter,
  AuthValidationState,
  AuthValidationTarget,
  Provider,
  ProviderKind,
} from '../../types/domain'

export function buildAuthValidationLookup(results: AuthValidationState['payload']['results']) {
  const byProvider = new Map<string, AuthValidationState['payload']['results']>()
  for (const result of results) {
    const key = `${result.providerIndex}`
    const items = byProvider.get(key) ?? []
    items.push(result)
    byProvider.set(key, items)
  }
  return byProvider
}

export function buildProviderKindIndexMap(providers: Provider[]) {
  const seen: Partial<Record<AuthProviderKind, number>> = {}
  const indices = new Map<string, number>()
  for (const provider of providers) {
    if (provider.kind !== 'codex' && provider.kind !== 'grok') continue
    const kind = provider.kind
    const index = seen[kind] ?? 0
    indices.set(provider.id, index)
    seen[kind] = index + 1
  }
  return indices
}

export function buildProviderOrderMap(providers: Provider[]) {
  const grouped = new Map<ProviderKind, Provider[]>()
  for (const provider of providers) {
    grouped.set(provider.kind, [...(grouped.get(provider.kind) ?? []), provider])
  }

  const order = new Map<string, { index: number; total: number }>()
  for (const group of grouped.values()) {
    for (const [index, provider] of group.entries()) {
      order.set(provider.id, { index, total: group.length })
    }
  }
  return order
}

export function providerAuthTargets(provider: Provider, providerIndex: number) {
  if (provider.kind !== 'codex' && provider.kind !== 'grok') return []
  if (providerIndex < 0) return []
  const auth = Array.isArray(provider.auth)
    ? provider.auth
    : provider.auth === undefined || provider.auth === null ? [] : [provider.auth]
  return auth.map((_, authIndex) => ({ providerIndex, authIndex }))
}

export function providerAuthStats(
  provider: Provider,
  providerIndex: number,
  authValidation: AuthValidationState | null,
  lookup: Map<string, AuthValidationState['payload']['results']>,
) {
  if (provider.kind !== 'codex' && provider.kind !== 'grok') {
    return undefined
  }
  const authItems = Array.isArray(provider.auth)
    ? provider.auth
    : provider.auth === undefined || provider.auth === null ? [] : [provider.auth]
  const results = authValidation?.kind === provider.kind ? lookup.get(`${providerIndex}`) ?? [] : []
  const disabled = authItems.filter((auth) => !provider.enabled || authValueDisabled(auth)).length
  const enabled = Math.max(0, authItems.length - disabled)
  const activeResults = results.filter((item) => !item.disabled && item.authIndex < authItems.length)
  const valid = activeResults.filter((item) => item.valid && item.reason !== 'rate_limited').length
  const invalid = activeResults.filter((item) => !item.valid && !item.skipped).length
  const rateLimited = activeResults.filter((item) => item.reason === 'rate_limited').length
  return {
    total: authItems.length,
    enabled,
    valid,
    invalid,
    rateLimited,
    disabled,
    unchecked: Math.max(0, enabled - valid - invalid - rateLimited),
  }
}

export function authValidationSummary(results: AuthValidationState['payload']['results']) {
  const total = results.length
  const disabled = results.filter((result) => result.disabled).length
  const enabled = Math.max(0, total - disabled)
  const enabledResults = results.filter((result) => !result.disabled)
  const valid = enabledResults.filter((result) => result.valid && result.reason !== 'rate_limited').length
  const invalid = enabledResults.filter((result) => !result.valid && !result.skipped).length
  const skipped = enabledResults.filter((result) => result.skipped).length
  return {
    total,
    enabled,
    disabled,
    valid,
    invalid,
    skipped,
    rateLimited: enabledResults.filter((result) => result.reason === 'rate_limited').length,
    refreshed: results.filter((result) => result.refreshed).length,
  }
}

export function visibleAuthValidationResults(state: AuthValidationState) {
  return state.payload.results.filter((result) => matchesAuthValidationFilter(result, state.filter))
}

export function authValidationFilterOptions(results: ReturnType<typeof visibleAuthValidationResults>) {
  const count = (filter: AuthValidationFilter) =>
    results.filter((result) => matchesAuthValidationFilter(result, filter)).length
  return [
    { value: 'all', label: '全部', count: results.length },
    { value: 'ok', label: '有效', count: count('ok') },
    { value: 'invalid', label: '无效', count: count('invalid') },
    { value: 'rate_limited', label: '限流', count: count('rate_limited') },
    { value: 'disabled', label: '禁用', count: count('disabled') },
  ] satisfies Array<{ value: AuthValidationFilter; label: string; count: number }>
}

export function AuthValidationResultRow({
  kind,
  result,
  disabled,
  onValidate,
  onDisable,
  onDelete,
}: {
  kind: AuthProviderKind
  result: AuthValidationState['payload']['results'][number]
  disabled: boolean
  onValidate: (kind: AuthProviderKind, target: AuthValidationTarget) => void
  onDisable: (kind: AuthProviderKind, target: AuthValidationTarget, disabled: boolean) => void
  onDelete: (kind: AuthProviderKind, target: AuthValidationTarget) => void
}) {
  const target = { providerIndex: result.providerIndex, authIndex: result.authIndex }
  const statusClass = result.reason === 'rate_limited'
    ? 'skipped'
    : result.valid ? 'ok' : result.skipped ? 'skipped' : 'error'
  return (
    <div className="auth-validation-result-row">
      <div className="auth-validation-result-main">
        <span className={`status-dot ${statusClass}`} />
        <div>
          <strong>{result.label}</strong>
          <span>
            配置 #{result.providerIndex + 1}
            {result.authCount > 1 ? ` / Auth #${result.authIndex + 1}` : ''}
          </span>
        </div>
      </div>
      <div className="auth-validation-result-detail">
        <code>{authValidationReasonLabel(result.reason)}</code>
        {result.statusCode > 0 && <span>HTTP {result.statusCode}</span>}
        {result.refreshed && <span>已刷新</span>}
        {result.reason === 'rate_limited' && result.disabled && <span>已自动禁用</span>}
        {result.disabled && <span>已禁用</span>}
        {result.errorMessage && <span className="auth-validation-error">{result.errorMessage}</span>}
      </div>
      <div className="auth-validation-row-actions">
        <button className="icon-button subtle" type="button" title="重新校验" disabled={disabled} onClick={() => onValidate(kind, target)}>
          <Icon name="check" size={15} />
        </button>
        <button className="button button-secondary compact-model-sync-button" type="button" onClick={() => onDisable(kind, target, !result.disabled)}>
          {result.disabled ? '启用' : '禁用'}
        </button>
        <button className="icon-button danger-button" type="button" title="删除 auth" onClick={() => onDelete(kind, target)}>
          <Icon name="trash" size={15} />
        </button>
      </div>
    </div>
  )
}
