import { useState } from 'react'
import { Icon } from '../../components/Icon'
import { providerMeta, providerMarkText, effectiveBaseUrlForProvider, defaultPriority } from '../../config/providers'
import type { ListMoveAction } from '../../lib/list'
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
  AuthValidationResultRow,
  authValidationFilterOptions,
  authValidationSummary,
  buildAuthValidationLookup,
  buildProviderKindIndexMap,
  buildProviderOrderMap,
  providerAuthStats,
  providerAuthTargets,
  visibleAuthValidationResults,
} from './authValidationView'

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
  onMoveProvider,
  onReorderProvider,
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
  onMoveProvider: (id: string, action: ListMoveAction) => void
  onReorderProvider: (sourceId: string, targetId: string) => void
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
  const [draggedProviderId, setDraggedProviderId] = useState<string | null>(null)
  const visibleProviders = providers.filter((provider) => {
    const matchesFilter = filter === 'all' || provider.enabled
    const matchesKind = kindFilter === 'all' || provider.kind === kindFilter
    const searchText = `${provider.name} ${providerMeta[provider.kind].label} ${provider.baseUrl} ${effectiveBaseUrlForProvider(provider)}`.toLowerCase()
    return matchesFilter && matchesKind && searchText.includes(query.toLowerCase())
  })
  const authValidationResults = authValidation ? visibleAuthValidationResults(authValidation) : []
  const authSummary = authValidation ? authValidationSummary(authValidation.payload.results) : null
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
  const providerOrder = buildProviderOrderMap(providers)

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
                {groupProviders.map((provider, index) => (
                  <ProviderCard
                    key={provider.id}
                    provider={provider}
                    priorityIndex={providerOrder.get(provider.id)?.index ?? index}
                    priorityTotal={providerOrder.get(provider.id)?.total ?? groupProviders.length}
                    isDragging={draggedProviderId === provider.id}
                    onToggle={onToggle}
                    onEdit={onEdit}
                    onCopy={onCopy}
                    onDelete={onDelete}
                    onMove={(direction) => onMoveProvider(provider.id, direction)}
                    onDragStart={() => setDraggedProviderId(provider.id)}
                    onDragEnd={() => setDraggedProviderId(null)}
                    onDrop={() => {
                      if (draggedProviderId) onReorderProvider(draggedProviderId, provider.id)
                      setDraggedProviderId(null)
                    }}
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
              <span className="summary total">总数 <b>{authSummary?.total ?? 0}</b></span>
              <span className="summary enabled">启用 <b>{authSummary?.enabled ?? 0}</b></span>
              <span className="summary disabled">禁用 <b>{authSummary?.disabled ?? 0}</b></span>
              <span className="status ok">有效 <b>{authSummary?.valid ?? 0}</b></span>
              <span className="status error">无效 <b>{authSummary?.invalid ?? 0}</b></span>
              <span className="status skipped">跳过 <b>{authSummary?.skipped ?? 0}</b></span>
              <span className="status limited">限流 <b>{authSummary?.rateLimited ?? 0}</b></span>
              <span>刷新 <b>{authSummary?.refreshed ?? 0}</b></span>
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
