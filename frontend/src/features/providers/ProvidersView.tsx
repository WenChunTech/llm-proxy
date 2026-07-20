import { Icon } from '../../components/Icon'
import { providerMeta, providerMarkText, effectiveBaseUrlForProvider, defaultPriority } from '../../config/providers'
import type {
  AuthProviderKind,
  AuthValidationFilter,
  AuthValidationState,
  AuthValidationTarget,
  Provider,
  ProviderKind,
  ProviderKindFilter,
} from '../../types/domain'
import { ProviderCard } from './ProviderCard'
import {
  authValueDisabled,
  authValidationReasonLabel,
  matchesAuthValidationFilter,
} from '../../lib/authValidation'

export function ProvidersView({
  providers,
  query,
  filter,
  kindFilter,
  onQueryChange,
  onFilterChange,
  onKindFilterChange,
  onToggle,
  onEdit,
  onCopy,
  onDelete,
  onAdd,
  onValidateAuths,
  validatingAuthKind,
  authValidation,
  onAuthValidationFilterChange,
  onValidateVisibleAuths,
  onEnableVisibleAuths,
  onDisableVisibleAuths,
  onDeleteVisibleAuths,
  onValidateAuthResult,
  onDisableAuthResult,
  onDeleteAuthResult,
}: {
  providers: Provider[]
  query: string
  filter: 'all' | 'enabled'
  kindFilter: ProviderKindFilter
  onQueryChange: (value: string) => void
  onFilterChange: (value: 'all' | 'enabled') => void
  onKindFilterChange: (value: ProviderKindFilter) => void
  onToggle: (id: string) => void
  onEdit: (provider: Provider) => void
  onCopy: (provider: Provider) => void
  onDelete: (id: string) => void
  onAdd: () => void
  onValidateAuths: (kind: AuthProviderKind, targets?: AuthValidationTarget[]) => void
  validatingAuthKind: ProviderKind | null
  authValidation: AuthValidationState | null
  onAuthValidationFilterChange: (filter: AuthValidationFilter) => void
  onValidateVisibleAuths: () => void
  onEnableVisibleAuths: () => void
  onDisableVisibleAuths: () => void
  onDeleteVisibleAuths: () => void
  onValidateAuthResult: (kind: AuthProviderKind, target: AuthValidationTarget) => void
  onDisableAuthResult: (kind: AuthProviderKind, target: AuthValidationTarget, disabled: boolean) => void
  onDeleteAuthResult: (kind: AuthProviderKind, target: AuthValidationTarget) => void
}) {
  const visibleProviders = providers.filter((provider) => {
    const matchesFilter = filter === 'all' || provider.enabled
    const matchesKind = kindFilter === 'all' || provider.kind === kindFilter
    const searchText = `${provider.name} ${providerMeta[provider.kind].label} ${provider.baseUrl} ${effectiveBaseUrlForProvider(provider)}`.toLowerCase()
    return matchesFilter && matchesKind && searchText.includes(query.toLowerCase())
  })
  const authValidationResults = authValidation ? visibleAuthValidationResults(authValidation) : []
  const groupedProviders = defaultPriority
    .map((kind) => ({
      kind,
      providers: visibleProviders.filter((provider) => provider.kind === kind),
    }))
      .filter((group) => group.providers.length)
  const authValidationByProvider = authValidation
    ? buildAuthValidationLookup(authValidation.payload.results)
    : new Map<string, AuthValidationState['payload']['results']>()
  const providerKindIndices = buildProviderKindIndexMap(providers)

  return (
    <>
      <section className="page-intro">
        <div>
          <span className="eyebrow">UPSTREAM CONNECTIONS</span>
          <h2>提供商配置</h2>
          <p>维护上游 API 端点、模型清单与启用状态。保存后 Rust 运行态会立即刷新路由。</p>
        </div>
        <button className="button button-primary" type="button" onClick={onAdd}><Icon name="plus" size={17} />添加提供商</button>
      </section>
      <div className="toolbar">
        <label className="search-field">
          <Icon name="search" size={17} />
          <input value={query} onChange={(event) => onQueryChange(event.target.value)} placeholder="搜索提供商、协议或地址" />
        </label>
        <div className="segmented-control">
          <button className={filter === 'all' ? 'selected' : ''} type="button" onClick={() => onFilterChange('all')}>全部 <span>{providers.length}</span></button>
          <button className={filter === 'enabled' ? 'selected' : ''} type="button" onClick={() => onFilterChange('enabled')}>已启用 <span>{providers.filter((provider) => provider.enabled).length}</span></button>
        </div>
      </div>
      {kindFilter !== 'all' && (
        <div className="active-filter-bar">
          <span>
            当前分组：<strong>{providerMeta[kindFilter].label}</strong>
          </span>
          <button className="text-button" type="button" onClick={() => onKindFilterChange('all')}>
            查看全部
            <Icon name="chevron" size={14} />
          </button>
        </div>
      )}
      <div className="provider-list">
        {groupedProviders.map(({ kind, providers: groupProviders }) => {
          const meta = providerMeta[kind]
          const enabledCount = groupProviders.filter((provider) => provider.enabled).length
          return (
            <section className="provider-group" key={kind}>
              <div className="provider-group-heading">
                <div className="provider-group-title">
                  <div className={`provider-avatar ${kind === 'grok' ? 'grok-avatar' : ''}`} style={{ backgroundColor: meta.color }}>{providerMarkText(kind)}</div>
                  <div>
                    <h3>{meta.label}</h3>
                    <span>{meta.description}</span>
                  </div>
                </div>
                <div className="provider-group-actions">
                  {(kind === 'codex' || kind === 'grok') && (
                    <button
                      className="button button-secondary compact-model-sync-button"
                      type="button"
                      disabled={validatingAuthKind === kind}
                      onClick={() => onValidateAuths(kind)}
                    >
                      <Icon name="check" size={15} />
                      {validatingAuthKind === kind ? '校验中' : '校验 Auth'}
                    </button>
                  )}
                  <span className="provider-group-count">{enabledCount}/{groupProviders.length} 已启用</span>
                </div>
              </div>
              <div className="provider-group-list">
                {groupProviders.map((provider) => (
                  <ProviderCard
                    key={provider.id}
                    provider={provider}
                    onToggle={onToggle}
                    onEdit={onEdit}
                    onCopy={onCopy}
                    onDelete={onDelete}
                    authStats={providerAuthStats(provider, providerKindIndices.get(provider.id) ?? -1, authValidation, authValidationByProvider)}
                    authTargets={providerAuthTargets(provider, providerKindIndices.get(provider.id) ?? -1)}
                    onValidateAuth={onValidateAuths}
                  />
                ))}
              </div>
            </section>
          )
        })}
        {!visibleProviders.length && <div className="empty-state"><Icon name="search" size={24} /><strong>没有匹配的提供商</strong><span>尝试修改搜索词或筛选条件。</span></div>}
      </div>
      {authValidation && (
        <section className="auth-validation-panel">
          <div className="auth-validation-summary">
            <div>
              <span className="eyebrow">AUTH VALIDATION</span>
              <strong>{providerMeta[authValidation.kind].label} 校验结果</strong>
            </div>
            <div className="auth-validation-metrics">
              <span>总数 {authValidation.payload.total}</span>
              <span>有效 {authValidation.payload.valid}</span>
              <span>无效 {authValidation.payload.invalid}</span>
              <span>限流 {authValidation.payload.rateLimited}</span>
              <span>禁用 {authValidation.payload.results.filter((result) => result.disabled).length}</span>
              <span>刷新 {authValidation.payload.refreshed}</span>
            </div>
          </div>
          <div className="auth-validation-toolbar">
            <div className="segmented-control auth-validation-filters">
              {authValidationFilterOptions(authValidation.payload.results).map((option) => (
                <button
                  className={authValidation.filter === option.value ? 'selected' : ''}
                  type="button"
                  key={option.value}
                  onClick={() => onAuthValidationFilterChange(option.value)}
                >
                  {option.label} <span>{option.count}</span>
                </button>
              ))}
            </div>
            <div className="auth-validation-actions">
              <button className="button button-secondary compact-model-sync-button" type="button" onClick={onValidateVisibleAuths} disabled={validatingAuthKind === authValidation.kind || !authValidationResults.length}>
                <Icon name="check" size={15} />
                校验当前筛选
              </button>
              <button className="button button-secondary compact-model-sync-button" type="button" onClick={onEnableVisibleAuths} disabled={!authValidationResults.length}>
                启用当前筛选
              </button>
              <button className="button button-secondary compact-model-sync-button" type="button" onClick={onDisableVisibleAuths} disabled={!authValidationResults.length}>
                禁用当前筛选
              </button>
              <button className="button button-secondary compact-model-sync-button danger-action" type="button" onClick={onDeleteVisibleAuths} disabled={!authValidationResults.length}>
                删除当前筛选
              </button>
            </div>
          </div>
          <div className="auth-validation-results">
            {authValidationResults.map((result) => (
              <AuthValidationResultRow
                key={`${result.providerIndex}:${result.authIndex}`}
                kind={authValidation.kind}
                result={result}
                disabled={validatingAuthKind === authValidation.kind}
                onValidate={onValidateAuthResult}
                onDisable={onDisableAuthResult}
                onDelete={onDeleteAuthResult}
              />
            ))}
            {!authValidationResults.length && (
              <div className="empty-state small"><span>当前筛选项没有结果</span></div>
            )}
          </div>
        </section>
      )}
    </>
  )
}

function buildAuthValidationLookup(results: AuthValidationState['payload']['results']) {
  const byProvider = new Map<string, AuthValidationState['payload']['results']>()
  for (const result of results) {
    const key = `${result.providerIndex}`
    const items = byProvider.get(key) ?? []
    items.push(result)
    byProvider.set(key, items)
  }
  return byProvider
}

function buildProviderKindIndexMap(providers: Provider[]) {
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

function providerAuthTargets(provider: Provider, providerIndex: number) {
  if (provider.kind !== 'codex' && provider.kind !== 'grok') return []
  if (providerIndex < 0) return []
  const auth = Array.isArray(provider.auth)
    ? provider.auth
    : provider.auth === undefined || provider.auth === null ? [] : [provider.auth]
  return auth.map((_, authIndex) => ({ providerIndex, authIndex }))
}

function providerAuthStats(
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
  const disabledFromConfig = authItems.filter((auth) => !provider.enabled || authValueDisabled(auth)).length
  const activeResults = results.filter((item) => !item.disabled)
  return {
    total: authItems.length,
    valid: activeResults.filter((item) => item.valid && item.reason !== 'rate_limited').length,
    invalid: activeResults.filter((item) => !item.valid && !item.skipped).length,
    disabled: disabledFromConfig,
  }
}

function visibleAuthValidationResults(state: AuthValidationState) {
  return state.payload.results.filter((result) => matchesAuthValidationFilter(result, state.filter))
}

function authValidationFilterOptions(results: ReturnType<typeof visibleAuthValidationResults>) {
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

function AuthValidationResultRow({
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
