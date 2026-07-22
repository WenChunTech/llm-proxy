import { Icon } from '../../components/Icon'
import { providerMeta, providerMarkText, effectiveBaseUrlForProvider } from '../../config/providers'
import type { ListMoveAction } from '../../lib/list'
import type { AuthProviderKind, AuthValidationTarget, Provider } from '../../types/domain'

type ProviderAuthStats = {
  total: number
  valid: number
  invalid: number
  disabled: number
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
}) {
  const meta = providerMeta[provider.kind]
  const effectiveBaseUrl = effectiveBaseUrlForProvider(provider)
  const canValidateAuth = Boolean(onValidateAuth && authTargets?.length && (provider.kind === 'codex' || provider.kind === 'grok'))
  const canReorder = Boolean(onMove && priorityTotal > 1)
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
          title={canReorder ? `拖动调整 ${provider.name} 优先级` : '仅一条配置时无需排序'}
          aria-label={canReorder ? `拖动调整 ${provider.name} 优先级` : '仅一条配置时无需排序'}
          disabled={!canReorder}
        >
          <Icon name="grip" size={15} />
        </button>
        <span className="priority-number">{String(priorityIndex + 1).padStart(2, '0')}</span>
        <div className="priority-actions provider-card-priority-actions">
          <button
            className="icon-button subtle"
            type="button"
            title="置顶"
            disabled={!canReorder || priorityIndex === 0}
            onClick={() => onMove?.('top')}
          >
            <Icon name="toTop" size={15} />
          </button>
          <button
            className="icon-button subtle"
            type="button"
            title="提高优先级"
            disabled={!canReorder || priorityIndex === 0}
            onClick={() => onMove?.(-1)}
          >
            <Icon name="arrowUp" size={15} />
          </button>
          <button
            className="icon-button subtle"
            type="button"
            title="降低优先级"
            disabled={!canReorder || priorityIndex >= priorityTotal - 1}
            onClick={() => onMove?.(1)}
          >
            <Icon name="arrowDown" size={15} />
          </button>
          <button
            className="icon-button subtle"
            type="button"
            title="置底"
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
            <span className={`status-badge ${provider.enabled ? 'enabled' : 'disabled'}`}>{provider.enabled ? '已启用' : '已停用'}</span>
          </div>
          <span className="provider-type">{meta.label} <i /> {meta.description}</span>
          <div className="provider-url"><Icon name="external" size={14} /><code>{effectiveBaseUrl || 'base_url 未配置'}</code></div>
          {authStats && (
            <div className="provider-auth-stats">
              <span>AUTH {authStats.total}</span>
              <span>有效 {authStats.valid}</span>
              <span>无效 {authStats.invalid}</span>
              <span>禁用 {authStats.disabled}</span>
            </div>
          )}
        </div>
      </div>
      <div className="provider-models">
        <span className="section-label">MODELS <b>{provider.models.length}</b></span>
        <div className="chip-list">
          {provider.models.map((model) => <span className="model-chip" key={model}>{model}</span>)}
          {!provider.models.length && <span className="muted-copy">尚未添加模型</span>}
        </div>
      </div>
      <div className="provider-card-actions">
        {canValidateAuth && authStats && authStats.total > 0 && (
          <button
            className="button button-secondary compact-model-sync-button"
            type="button"
            onClick={() => onValidateAuth?.(provider.kind as AuthProviderKind, authTargets ?? [])}
            title="校验当前配置"
          >
            <Icon name="check" size={15} />
            校验
          </button>
        )}
        <button className={`toggle ${provider.enabled ? 'on' : ''}`} type="button" aria-label={provider.enabled ? '停用提供商' : '启用提供商'} onClick={() => onToggle(provider.id)}><span /></button>
        <button className="icon-button" type="button" title="编辑提供商" onClick={() => onEdit(provider)}><Icon name="edit" size={16} /></button>
        <button className="icon-button" type="button" title="复制配置" onClick={() => onCopy(provider)}><Icon name="copy" size={16} /></button>
        <button className="icon-button danger-button" type="button" title="删除提供商" onClick={() => onDelete(provider.id)}><Icon name="trash" size={16} /></button>
      </div>
    </article>
  )
}
