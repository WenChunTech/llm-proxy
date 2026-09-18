import { Icon } from '../../components/Icon'
import { useI18n } from '../../lib/i18n'
import { providerMeta, providerMarkText, effectiveBaseUrlForProvider } from '../../config/providers'
import type { ListMoveAction } from '../../lib/list'
import type { AuthProviderKind, AuthValidationTarget, Provider } from '../../types/domain'

type ProviderAuthStats = {
  total: number
  enabled: number
  valid: number
  invalid: number
  rateLimited: number
  disabled: number
  unchecked: number
}

export function ProviderCard({
  provider,
  priorityIndex = 0,
  priorityTotal = 1,
  isDragging = false,
  onToggle,
  onEdit,
  onCopy,
  onDelete,
  onMove,
  onDragStart,
  onDragEnd,
  onDrop,
  authStats,
  authTargets,
  onValidateAuth,
  authValidating = false,
  authProgress = null,
}: {
  provider: Provider
  priorityIndex?: number
  priorityTotal?: number
  isDragging?: boolean
  onToggle: (id: string) => void
  onEdit: (provider: Provider) => void
  onCopy: (provider: Provider) => void
  onDelete: (id: string) => void
  onMove?: (action: ListMoveAction) => void
  onDragStart?: () => void
  onDragEnd?: () => void
  onDrop?: () => void
  authStats?: ProviderAuthStats
  authTargets?: AuthValidationTarget[]
  onValidateAuth?: (kind: AuthProviderKind, targets: AuthValidationTarget[]) => void
  authValidating?: boolean
  authProgress?: { completed: number; total: number; label: string } | null
}) {
  const { t } = useI18n()
  const meta = providerMeta[provider.kind]
  const effectiveBaseUrl = effectiveBaseUrlForProvider(provider)
  const canValidateAuth = Boolean(onValidateAuth && authTargets?.length && (provider.kind === 'codex' || provider.kind === 'grok'))
  const canReorder = Boolean(onMove && priorityTotal > 1)
  const enabledAuthPercent = authStats && authStats.total > 0
    ? `${(authStats.enabled / authStats.total) * 100}%`
    : '0%'
  const disabledAuthPercent = authStats && authStats.total > 0
    ? `${(authStats.disabled / authStats.total) * 100}%`
    : '0%'
  return (
    <article
      className={`provider-card ${provider.enabled ? '' : 'is-disabled'} ${isDragging ? 'is-dragging' : ''} ${canReorder ? 'is-reorderable' : ''}`}
      draggable={canReorder}
      onDragStart={(event) => {
        if (!canReorder) return
        event.dataTransfer.effectAllowed = 'move'
        event.dataTransfer.setData('text/plain', provider.id)
        onDragStart?.()
      }}
      onDragOver={(event) => {
        if (!canReorder) return
        event.preventDefault()
        event.dataTransfer.dropEffect = 'move'
      }}
      onDrop={(event) => {
        if (!canReorder) return
        event.preventDefault()
        onDrop?.()
      }}
      onDragEnd={() => onDragEnd?.()}
    >
      <div className="provider-card-priority">
        <button
          className="priority-drag-handle"
          type="button"
          title={canReorder ? t('card.dragPriority', { name: provider.name }) : t('card.noReorder')}
          aria-label={canReorder ? t('card.dragPriority', { name: provider.name }) : t('card.noReorder')}
          disabled={!canReorder}
        >
          <Icon name="grip" size={15} />
        </button>
        <span className="priority-number">{String(priorityIndex + 1).padStart(2, '0')}</span>
        <div className="priority-actions provider-card-priority-actions">
          <button
            className="icon-button subtle"
            type="button"
            title={t('card.toTop')}
            disabled={!canReorder || priorityIndex === 0}
            onClick={() => onMove?.('top')}
          >
            <Icon name="toTop" size={15} />
          </button>
          <button
            className="icon-button subtle"
            type="button"
            title={t('card.raise')}
            disabled={!canReorder || priorityIndex === 0}
            onClick={() => onMove?.(-1)}
          >
            <Icon name="arrowUp" size={15} />
          </button>
          <button
            className="icon-button subtle"
            type="button"
            title={t('card.lower')}
            disabled={!canReorder || priorityIndex >= priorityTotal - 1}
            onClick={() => onMove?.(1)}
          >
            <Icon name="arrowDown" size={15} />
          </button>
          <button
            className="icon-button subtle"
            type="button"
            title={t('card.toBottom')}
            disabled={!canReorder || priorityIndex >= priorityTotal - 1}
            onClick={() => onMove?.('bottom')}
          >
            <Icon name="toBottom" size={15} />
          </button>
        </div>
      </div>
      <div className="provider-card-main">
        <div className={`provider-avatar large ${provider.kind === 'grok' ? 'grok-avatar' : ''}`} style={{ backgroundColor: meta.color }}>{providerMarkText(provider.kind)}</div>
        <div className="provider-card-copy">
          <div className="provider-title-row">
            <h3>{provider.name}</h3>
            <span className={`status-badge ${provider.enabled ? 'enabled' : 'disabled'}`}>{provider.enabled ? t('card.enabled') : t('card.disabled')}</span>
          </div>
          <span className="provider-type">{meta.label} <i /> {meta.description}</span>
          <div className="provider-url"><Icon name="external" size={14} /><code>{effectiveBaseUrl || t('card.baseUrlNotConfigured')}</code></div>
          {authStats && (
            <div className="provider-auth-overview">
              <div className="provider-auth-total-row">
                <span>AUTH <b>{authStats.total}</b></span>
                <code>{t('card.authSummary', { enabled: authStats.enabled, disabled: authStats.disabled })}</code>
              </div>
              <div className="provider-auth-meter" aria-hidden="true">
                <span className="enabled" style={{ width: enabledAuthPercent }} />
                <span className="disabled" style={{ width: disabledAuthPercent }} />
              </div>
              <div className="provider-auth-stats">
                <span className="auth-stat enabled">{t('card.enabled')} <b>{authStats.enabled}</b></span>
                <span className="auth-stat valid">{t('card.valid')} <b>{authStats.valid}</b></span>
                <span className="auth-stat invalid">{t('card.invalid')} <b>{authStats.invalid}</b></span>
                {authStats.rateLimited > 0 && <span className="auth-stat limited">{t('card.rateLimited')} <b>{authStats.rateLimited}</b></span>}
                {authStats.unchecked > 0 && <span className="auth-stat unchecked">{t('card.unchecked')} <b>{authStats.unchecked}</b></span>}
                <span className="auth-stat disabled">{t('card.disabledLabel')} <b>{authStats.disabled}</b></span>
              </div>
              {authValidating && (
                <div className="provider-auth-progress" aria-live="polite">
                  <div className="provider-auth-progress-row">
                    <strong>{t('card.validating')}</strong>
                    <code>
                      {authProgress && authProgress.total > 0
                        ? `${authProgress.completed}/${authProgress.total}`
                        : '…'}
                    </code>
                  </div>
                  <div className="provider-auth-progress-meter" aria-hidden="true">
                    <span
                      style={{
                        width:
                          authProgress && authProgress.total > 0
                            ? `${Math.min(100, (authProgress.completed / authProgress.total) * 100)}%`
                            : '12%',
                      }}
                    />
                  </div>
                  {authProgress?.label ? (
                    <span className="provider-auth-progress-label">{authProgress.label}</span>
                  ) : null}
                </div>
              )}
            </div>
          )}
        </div>
      </div>
      <div className="provider-models">
        <span className="section-label">MODELS <b>{provider.models.length}</b></span>
        <div className="chip-list">
          {provider.models.map((model) => <span className="model-chip" key={model}>{model}</span>)}
          {!provider.models.length && <span className="muted-copy">{t('card.noModels')}</span>}
        </div>
      </div>
      <div className="provider-card-actions">
        {canValidateAuth && authStats && authStats.total > 0 && (
          <button
            className="button button-secondary compact-model-sync-button"
            type="button"
            disabled={authValidating}
            onClick={() => onValidateAuth?.(provider.kind as AuthProviderKind, authTargets ?? [])}
            title={authValidating ? t('card.validatingInProgress') : t('card.validateConfig')}
          >
            <Icon name={authValidating ? 'pulse' : 'check'} size={15} />
            {authValidating
              ? authProgress && authProgress.total > 0
                ? `${authProgress.completed}/${authProgress.total}`
                : t('card.validating')
              : t('card.validate')}
          </button>
        )}
        <button className={`toggle ${provider.enabled ? 'on' : ''}`} type="button" aria-label={provider.enabled ? t('card.disableProvider') : t('card.enableProvider')} onClick={() => onToggle(provider.id)}><span /></button>
        <button className="icon-button" type="button" title={t('card.editProvider')} onClick={() => onEdit(provider)}><Icon name="edit" size={16} /></button>
        <button className="icon-button provider-clone-button" type="button" title={t('card.cloneProvider')} aria-label={t('card.cloneProvider')} onClick={() => onCopy(provider)}><Icon name="copy" size={16} /></button>
        <button className="icon-button danger-button" type="button" title={t('card.deleteProvider')} onClick={() => onDelete(provider.id)}><Icon name="trash" size={16} /></button>
      </div>
    </article>
  )
}
