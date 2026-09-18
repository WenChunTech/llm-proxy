import { useEffect, useState } from 'react'
import { Icon } from '../../components/Icon'
import { useI18n } from '../../lib/i18n'
import { providerMeta, providerMarkText, effectiveBaseUrlForProvider, defaultPriority } from '../../config/providers'
import type { ListMoveAction } from '../../lib/list'
import type {
  AuthProviderKind,
  AuthValidationFilter,
  AuthValidationState,
  AuthValidationTarget,
  Provider,
  ProviderKindFilter,
} from '../../types/domain'
import { ProviderCard } from './ProviderCard'
import { AuthValidationResultRow } from './authValidationView'
import {
  authValidationFilterOptions,
  authValidationSummary,
  buildAuthValidationLookup,
  buildProviderKindIndexMap,
  buildProviderOrderMap,
  providerAuthStats,
  providerAuthTargets,
  visibleAuthValidationResults,
} from './authValidationViewData'

const AUTH_VALIDATION_CONCURRENCY_MIN = 1

function parseAuthValidationConcurrency(value: string) {
  const next = Number.parseInt(value, 10)
  return Number.isFinite(next) && next >= AUTH_VALIDATION_CONCURRENCY_MIN ? next : null
}

function AuthValidationConcurrencyControl({
  value,
  disabled,
  compact = false,
  onChange,
}: {
  value: number
  disabled: boolean
  compact?: boolean
  onChange: (value: number) => void
}) {
  const { t } = useI18n()
  const [draft, setDraft] = useState(String(value))

  useEffect(() => {
    setDraft(String(value))
  }, [value])

  function commitDraft() {
    const next = parseAuthValidationConcurrency(draft)
    if (next !== null) {
      onChange(next)
      return
    }
    setDraft(String(value))
  }

  return (
    <label
      className={`auth-validation-concurrency-control${compact ? ' compact' : ''}`}
      title={t('providers.concurrencyTitle')}
    >
      <span>{compact ? t('providers.concurrencyShort') : t('providers.concurrencyFull')}</span>
      <input
        type="text"
        inputMode="numeric"
        pattern="[0-9]*"
        aria-label={t('providers.concurrencyAria')}
        value={draft}
        disabled={disabled}
        onChange={(event) => {
          const next = event.currentTarget.value
          if (/^\d*$/.test(next)) setDraft(next)
        }}
        onBlur={commitDraft}
        onKeyDown={(event) => {
          if (event.key !== 'Enter') return
          commitDraft()
          event.currentTarget.blur()
        }}
      />
    </label>
  )
}

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
  isKindValidating,
  isProviderValidating,
  isTargetValidating,
  providerValidationProgress,
  authValidation,
  validationConcurrency,
  onValidationConcurrencyChange,
  onAuthValidationFilterChange,
  onClearAuthValidation,
  onValidateVisibleAuths,
  onEnableVisibleAuths,
  onDisableVisibleAuths,
  onDeleteVisibleAuths,
  onValidateAuthResult,
  onDisableAuthResult,
  onDeleteAuthResult,
  setToast,
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
  isKindValidating: (kind: AuthProviderKind) => boolean
  isProviderValidating: (kind: AuthProviderKind, providerIndex: number) => boolean
  isTargetValidating: (kind: AuthProviderKind, target: AuthValidationTarget) => boolean
  providerValidationProgress: (
    kind: AuthProviderKind,
    providerIndex: number,
  ) => { completed: number; total: number; label: string } | null
  authValidation: AuthValidationState | null
  validationConcurrency: number
  onValidationConcurrencyChange: (value: number) => void
  onAuthValidationFilterChange: (filter: AuthValidationFilter) => void
  onClearAuthValidation: () => void
  onValidateVisibleAuths: () => void
  onEnableVisibleAuths: () => void
  onDisableVisibleAuths: () => void
  onDeleteVisibleAuths: () => void
  onValidateAuthResult: (kind: AuthProviderKind, target: AuthValidationTarget) => void
  onDisableAuthResult: (kind: AuthProviderKind, target: AuthValidationTarget, disabled: boolean) => void
  onDeleteAuthResult: (kind: AuthProviderKind, target: AuthValidationTarget) => void
  setToast: (message: string) => void
}) {
  const { t } = useI18n()
  const [draggedProviderId, setDraggedProviderId] = useState<string | null>(null)
  const visibleProviders = providers.filter((provider) => {
    const matchesFilter = filter === 'all' || provider.enabled
    const matchesKind = kindFilter === 'all' || provider.kind === kindFilter
    const searchText = `${provider.name} ${providerMeta[provider.kind].label} ${effectiveBaseUrlForProvider(provider)} ${provider.models.join(' ')}`.toLowerCase()
    return matchesFilter && matchesKind && searchText.includes(query.trim().toLowerCase())
  })

  const groupedProviders = defaultPriority
    .map((kind) => ({
      kind,
      providers: visibleProviders.filter((provider) => provider.kind === kind),
    }))
    .filter((group) => group.providers.length)

  const providerKindIndices = buildProviderKindIndexMap(providers)
  const providerOrder = buildProviderOrderMap(providers)
  const authValidationByProvider = authValidation
    ? buildAuthValidationLookup(authValidation.payload.results)
    : new Map<string, AuthValidationState['payload']['results']>()
  const authSummary = authValidation ? authValidationSummary(authValidation.payload.results) : null
  const authValidationResults = authValidation
    ? visibleAuthValidationResults(authValidation)
    : []
  const showAuthValidationPanel = Boolean(authValidation && authValidationResults.length)

  return (
    <>
      <section className="page-intro">
        <div>
          <span className="eyebrow">{t('providers.eyebrow')}</span>
          <h2>{t('providers.configTitle')}</h2>
          <p>{t('providers.configDesc')}</p>
        </div>
        <div className="page-intro-actions">
          <AuthValidationConcurrencyControl
            value={validationConcurrency}
            disabled={isKindValidating('codex') || isKindValidating('grok')}
            onChange={onValidationConcurrencyChange}
          />
          <button className="button button-primary" type="button" onClick={onAdd}><Icon name="plus" size={17} />{t('providers.addProvider')}</button>
        </div>
      </section>
      <div className="toolbar">
        <label className="search-field">
          <Icon name="search" size={17} />
          <input value={query} onChange={(event) => onQueryChange(event.target.value)} placeholder={t('providers.searchPlaceholder')} />
        </label>
        <div className="segmented-control">
          <button className={filter === 'all' ? 'selected' : ''} type="button" onClick={() => onFilterChange('all')}>{t('providers.filterAll')} <span>{providers.length}</span></button>
          <button className={filter === 'enabled' ? 'selected' : ''} type="button" onClick={() => onFilterChange('enabled')}>{t('providers.filterEnabled')} <span>{providers.filter((provider) => provider.enabled).length}</span></button>
        </div>
      </div>
      {kindFilter !== 'all' && (
        <div className="active-filter-bar">
          <span>
            {t('providers.currentGroup')}<strong>{providerMeta[kindFilter].label}</strong>
          </span>
          <button className="text-button" type="button" onClick={() => onKindFilterChange('all')}>
            {t('providers.viewAll')}
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
                      disabled={isKindValidating(kind)}
                      onClick={() => onValidateAuths(kind)}
                    >
                      <Icon name={isKindValidating(kind) ? 'pulse' : 'check'} size={15} />
                      {isKindValidating(kind) ? t('providers.validating') : t('providers.validateAuth')}
                    </button>
                  )}
                  <span className="provider-group-count">{t('providers.enabledCount', { enabled: enabledCount, total: groupProviders.length })}</span>
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
                    authValidating={
                      (provider.kind === 'codex' || provider.kind === 'grok') &&
                      isProviderValidating(
                        provider.kind,
                        providerKindIndices.get(provider.id) ?? -1,
                      )
                    }
                    authProgress={
                      provider.kind === 'codex' || provider.kind === 'grok'
                        ? providerValidationProgress(
                            provider.kind,
                            providerKindIndices.get(provider.id) ?? -1,
                          )
                        : null
                    }
                  />
                ))}
              </div>
            </section>
          )
        })}
        {!visibleProviders.length && <div className="empty-state"><Icon name="search" size={24} /><strong>{t('providers.noMatch')}</strong><span>{t('providers.noMatchHint')}</span></div>}
      </div>
      {authValidation && showAuthValidationPanel && (
        <section className="auth-validation-panel">
          <div className="auth-validation-summary">
            <div>
              <span className="eyebrow">AUTH VALIDATION</span>
              <strong>{t('providers.validationResults', { label: providerMeta[authValidation.kind].label })}</strong>
            </div>
            <div className="auth-validation-summary-side">
              <AuthValidationConcurrencyControl
                value={validationConcurrency}
                disabled={isKindValidating(authValidation.kind)}
                compact
                onChange={onValidationConcurrencyChange}
              />
              <button
                className="icon-button"
                type="button"
                title={t('providers.closeValidation')}
                aria-label={t('providers.closeValidation')}
                onClick={onClearAuthValidation}
                disabled={isKindValidating(authValidation.kind)}
              >
                <Icon name="close" size={15} />
              </button>
            </div>
            <div className="auth-validation-metrics">
              <span className="summary total">{t('providers.total')} <b>{authSummary?.total ?? 0}</b></span>
              <span className="summary enabled">{t('providers.enabled')} <b>{authSummary?.enabled ?? 0}</b></span>
              <span className="summary disabled">{t('providers.disabled')} <b>{authSummary?.disabled ?? 0}</b></span>
              <span className="status ok">{t('providers.valid')} <b>{authSummary?.valid ?? 0}</b></span>
              <span className="status error">{t('providers.invalid')} <b>{authSummary?.invalid ?? 0}</b></span>
              <span className="status skipped">{t('providers.skipped')} <b>{authSummary?.skipped ?? 0}</b></span>
              <span className="status limited">{t('providers.rateLimited')} <b>{authSummary?.rateLimited ?? 0}</b></span>
              <span>{t('providers.refreshed')} <b>{authSummary?.refreshed ?? 0}</b></span>
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
              <button className="button button-secondary compact-model-sync-button" type="button" onClick={onValidateVisibleAuths} disabled={isKindValidating(authValidation.kind) || !authValidationResults.length}>
                <Icon name={isKindValidating(authValidation.kind) ? 'pulse' : 'check'} size={15} />
                {isKindValidating(authValidation.kind) ? t('providers.validating') : t('providers.validateFiltered')}
              </button>
              <button className="button button-secondary compact-model-sync-button" type="button" onClick={onEnableVisibleAuths} disabled={!authValidationResults.length}>
                {t('providers.enableFiltered')}
              </button>
              <button className="button button-secondary compact-model-sync-button" type="button" onClick={onDisableVisibleAuths} disabled={!authValidationResults.length}>
                {t('providers.disableFiltered')}
              </button>
              <button className="button button-secondary compact-model-sync-button danger-action" type="button" onClick={onDeleteVisibleAuths} disabled={!authValidationResults.length}>
                {t('providers.deleteFiltered')}
              </button>
            </div>
          </div>
          <div className="auth-validation-results">
            {authValidationResults.map((result) => (
              <AuthValidationResultRow
                key={`${result.providerIndex}:${result.authIndex}`}
                kind={authValidation.kind}
                result={result}
                disabled={isTargetValidating(authValidation.kind, {
                  providerIndex: result.providerIndex,
                  authIndex: result.authIndex,
                })}
                validating={isTargetValidating(authValidation.kind, {
                  providerIndex: result.providerIndex,
                  authIndex: result.authIndex,
                })}
                onValidate={onValidateAuthResult}
                onDisable={onDisableAuthResult}
                onDelete={onDeleteAuthResult}
                setToast={setToast}
              />
            ))}
            {!authValidationResults.length && (
              <div className="empty-state small"><span>{t('providers.noFilteredResults')}</span></div>
            )}
          </div>
        </section>
      )}
    </>
  )
}
