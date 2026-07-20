import { Icon } from '../../components/Icon'
import { providerMeta, providerMarkText, effectiveBaseUrlForProvider } from '../../config/providers'
import type { AuthProviderKind, AuthValidationTarget, Provider } from '../../types/domain'

type ProviderAuthStats = {
  total: number
  valid: number
  invalid: number
  disabled: number
}

export function ProviderCard({
  provider,
  onToggle,
  onEdit,
  onCopy,
  onDelete,
  authStats,
  authTargets,
  onValidateAuth,
}: {
  provider: Provider
  onToggle: (id: string) => void
  onEdit: (provider: Provider) => void
  onCopy: (provider: Provider) => void
  onDelete: (id: string) => void
  authStats?: ProviderAuthStats
  authTargets?: AuthValidationTarget[]
  onValidateAuth?: (kind: AuthProviderKind, targets: AuthValidationTarget[]) => void
}) {
  const meta = providerMeta[provider.kind]
  const effectiveBaseUrl = effectiveBaseUrlForProvider(provider)
  const canValidateAuth = Boolean(onValidateAuth && authTargets?.length && (provider.kind === 'codex' || provider.kind === 'grok'))
  return (
    <article className={`provider-card ${provider.enabled ? '' : 'is-disabled'}`}>
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
          {provider.models.slice(0, 4).map((model) => <span className="model-chip" key={model}>{model}</span>)}
          {provider.models.length > 4 && <span className="model-chip more-chip">+{provider.models.length - 4}</span>}
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
