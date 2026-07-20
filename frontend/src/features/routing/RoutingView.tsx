import { Icon } from '../../components/Icon'
import { defaultPriority, providerMeta, providerMarkText } from '../../config/providers'
import type { Provider, ProviderKind } from '../../types/domain'
import { useState } from 'react'

export function RoutingView({
  priority,
  fallbacks,
  allModels,
  modelsByProviderKind,
  providers,
  onMove,
  onReorder,
  onRemoveFallback,
  onMoveFallback,
  onReorderFallback,
  onAddFallbackModel,
  onAddFallback,
}: {
  priority: ProviderKind[]
  fallbacks: string[]
  allModels: string[]
  modelsByProviderKind: Record<ProviderKind, string[]>
  providers: Provider[]
  onMove: (index: number, direction: -1 | 1) => void
  onReorder: (sourceIndex: number, targetIndex: number) => void
  onRemoveFallback: (model: string) => void
  onMoveFallback: (index: number, direction: -1 | 1) => void
  onReorderFallback: (sourceIndex: number, targetIndex: number) => void
  onAddFallbackModel: (model: string) => void
  onAddFallback: () => void
}) {
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null)
  const [draggedFallbackIndex, setDraggedFallbackIndex] = useState<number | null>(null)
  const selectableModelGroups = defaultPriority
    .map((kind) => ({
      kind,
      models: modelsByProviderKind[kind].filter((model) => !fallbacks.includes(model)),
    }))
    .filter((group) => group.models.length)

  return (
    <>
      <section className="page-intro">
        <div>
          <span className="eyebrow">TRAFFIC ORCHESTRATION</span>
          <h2>模型路由策略</h2>
          <p>调整提供商尝试顺序，并为常用模型定义故障转移链。</p>
        </div>
        <div className="route-summary"><span className="status-dot" />自动路由已启用</div>
      </section>
      <div className="content-grid routing-grid">
        <section className="panel priority-panel">
          <div className="panel-heading">
            <div><span className="eyebrow">PROVIDER PRIORITY</span><h3>提供商优先级</h3></div>
            <span className="panel-caption">优先尝试顺序</span>
          </div>
          <div className="priority-list">
            {priority.map((kind, index) => {
              const meta = providerMeta[kind]
              const count = providers.filter((provider) => provider.kind === kind && provider.enabled).length
              return (
                <div
                  className={`priority-row ${draggedIndex === index ? 'is-dragging' : ''}`}
                  key={kind}
                  draggable
                  onDragStart={(event) => {
                    setDraggedIndex(index)
                    event.dataTransfer.effectAllowed = 'move'
                    event.dataTransfer.setData('text/plain', kind)
                  }}
                  onDragOver={(event) => {
                    event.preventDefault()
                    event.dataTransfer.dropEffect = 'move'
                  }}
                  onDrop={(event) => {
                    event.preventDefault()
                    if (draggedIndex !== null) onReorder(draggedIndex, index)
                    setDraggedIndex(null)
                  }}
                  onDragEnd={() => setDraggedIndex(null)}
                >
                  <button className="priority-drag-handle" type="button" title={`拖动调整 ${meta.label} 顺序`} aria-label={`拖动调整 ${meta.label} 顺序`}>
                    <Icon name="grip" size={16} />
                  </button>
                  <span className="priority-number">{String(index + 1).padStart(2, '0')}</span>
                  <div className={`provider-avatar ${kind === 'grok' ? 'grok-avatar' : ''}`} style={{ backgroundColor: meta.color }}>{providerMarkText(kind)}</div>
                  <div className="priority-copy"><strong>{meta.label}</strong><span>{count ? `${count} 个活动配置` : '暂无活动配置'}</span></div>
                  <div className="priority-actions">
                    <button className="icon-button subtle" type="button" title="上移" disabled={index === 0} onClick={() => onMove(index, -1)}><Icon name="arrowUp" size={15} /></button>
                    <button className="icon-button subtle" type="button" title="下移" disabled={index === priority.length - 1} onClick={() => onMove(index, 1)}><Icon name="arrowDown" size={15} /></button>
                  </div>
                </div>
              )
            })}
          </div>
          <div className="info-callout"><Icon name="pulse" size={16} /><span>同一模型会按照上面的顺序依次尝试，单个提供商可配置多个上游端点。</span></div>
        </section>
        <section className="panel fallback-panel">
          <div className="panel-heading">
            <div><span className="eyebrow">FALLBACK MODELS</span><h3>备用模型链</h3></div>
            <button className="button button-secondary fallback-add-button" type="button" onClick={onAddFallback}><Icon name="plus" size={16} />添加模型</button>
          </div>
          <p className="panel-description">主模型不可用时，按照以下顺序自动切换。</p>
          <div className="fallback-list">
            {fallbacks.map((model, index) => (
              <div
                className={`fallback-row ${draggedFallbackIndex === index ? 'is-dragging' : ''}`}
                key={model}
                draggable
                onDragStart={(event) => {
                  setDraggedFallbackIndex(index)
                  event.dataTransfer.effectAllowed = 'move'
                  event.dataTransfer.setData('text/plain', model)
                }}
                onDragOver={(event) => {
                  event.preventDefault()
                  event.dataTransfer.dropEffect = 'move'
                }}
                onDrop={(event) => {
                  event.preventDefault()
                  if (draggedFallbackIndex !== null) onReorderFallback(draggedFallbackIndex, index)
                  setDraggedFallbackIndex(null)
                }}
                onDragEnd={() => setDraggedFallbackIndex(null)}
              >
                <button className="priority-drag-handle" type="button" title={`拖动调整 ${model} 顺序`} aria-label={`拖动调整 ${model} 顺序`}>
                  <Icon name="grip" size={15} />
                </button>
                <span className="fallback-index">{index + 1}</span>
                <div className="fallback-copy"><strong>{model}</strong><span>{allModels.includes(model) ? '已注册模型' : '等待提供商配置'}</span></div>
                <div className="priority-actions">
                  <button className="icon-button subtle" type="button" title="上移" disabled={index === 0} onClick={() => onMoveFallback(index, -1)}><Icon name="arrowUp" size={15} /></button>
                  <button className="icon-button subtle" type="button" title="下移" disabled={index === fallbacks.length - 1} onClick={() => onMoveFallback(index, 1)}><Icon name="arrowDown" size={15} /></button>
                  <button className="icon-button subtle danger-button" type="button" title="移除备用模型" onClick={() => onRemoveFallback(model)}><Icon name="trash" size={15} /></button>
                </div>
              </div>
            ))}
          </div>
          {!fallbacks.length && <div className="empty-state small"><span>还没有备用模型</span></div>}
          <div className="model-catalog">
            <span className="section-label">AVAILABLE MODELS</span>
            {selectableModelGroups.map(({ kind, models }) => (
              <div className="model-catalog-group" key={kind}>
                <div className="model-catalog-heading">
                  <span className={`provider-nav-mark ${kind === 'grok' ? 'grok-mark' : ''}`} style={{ backgroundColor: providerMeta[kind].color }} />
                  <strong>{providerMeta[kind].label}</strong>
                  <small>{models.length}</small>
                </div>
                <div className="model-catalog-list">
                  {models.map((model) => (
                    <button className="model-pick-button" type="button" key={model} onClick={() => onAddFallbackModel(model)}>
                      {model}
                    </button>
                  ))}
                </div>
              </div>
            ))}
            {!selectableModelGroups.length && (
              <div className="empty-state small"><span>没有可添加的同步模型</span></div>
            )}
          </div>
        </section>
      </div>
    </>
  )
}
