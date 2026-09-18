import { Icon } from '../../components/Icon'
import { SelectControl } from '../../components/controls/SelectControl'
import { defaultPriority, providerMeta, providerMarkText } from '../../config/providers'
import { moveItemByAction, reorderItem } from '../../lib/list'
import type { ListMoveAction } from '../../lib/list'
import type { Provider, ProviderKind } from '../../types/domain'
import { useState } from 'react'
import { useI18n } from '../../lib/i18n'

export function RoutingView({
  priority,
  fallbacks,
  allModels,
  configuredModels,
  modelsByProviderKind,
  modelAliases,
  providers,
  onMove,
  onReorder,
  onRemoveFallback,
  onMoveFallback,
  onReorderFallback,
  onAddFallbackModel,
  onAddFallback,
  onUpdateModelAliases,
}: {
  priority: ProviderKind[]
  fallbacks: string[]
  allModels: string[]
  configuredModels: string[]
  modelsByProviderKind: Record<ProviderKind, string[]>
  modelAliases: Record<string, string[]>
  providers: Provider[]
  onMove: (index: number, action: ListMoveAction) => void
  onReorder: (sourceIndex: number, targetIndex: number) => void
  onRemoveFallback: (model: string) => void
  onMoveFallback: (index: number, action: ListMoveAction) => void
  onReorderFallback: (sourceIndex: number, targetIndex: number) => void
  onAddFallbackModel: (model: string) => void
  onAddFallback: () => void
  onUpdateModelAliases: (
    aliases:
      | Record<string, string[]>
      | ((current: Record<string, string[]>) => Record<string, string[]>),
  ) => void
}) {
  const { t } = useI18n()
  const [draggedIndex, setDraggedIndex] = useState<number | null>(null)
  const [draggedFallbackIndex, setDraggedFallbackIndex] = useState<number | null>(null)
  const [draggedAliasTarget, setDraggedAliasTarget] = useState<{ source: string; index: number } | null>(null)
  const [aliasSource, setAliasSource] = useState('')
  const [aliasTarget, setAliasTarget] = useState('')
  const configuredModelOptions = configuredModels.map((model) => ({ value: model, label: model }))
  const aliasSourceOptions = [
    { value: '', label: t('routing.selectPrimary') },
    ...configuredModelOptions,
  ]
  const existingTargets = modelAliases[aliasSource] ?? []
  const aliasTargetOptions = [
    { value: '', label: t('routing.selectFallback') },
    ...configuredModelOptions.filter(
      (option) => option.value !== aliasSource && !existingTargets.includes(option.value),
    ),
  ]
  const aliasRows = Object.entries(modelAliases)
    .map(([source, targets]) => [source, targets] as const)
    .sort(([a], [b]) => a.localeCompare(b))
  const aliasTargetCount = aliasRows.reduce((count, [, targets]) => count + targets.length, 0)
  const canAddAlias =
    Boolean(aliasSource) &&
    Boolean(aliasTarget) &&
    aliasSource !== aliasTarget &&
    !existingTargets.includes(aliasTarget) &&
    configuredModels.length >= 2
  const selectableModelGroups = defaultPriority
    .map((kind) => ({
      kind,
      models: modelsByProviderKind[kind].filter((model) => !fallbacks.includes(model)),
    }))
    .filter((group) => group.models.length)

  function addAlias() {
    const source = aliasSource.trim()
    const target = aliasTarget.trim()
    if (!source || !target || source === target) return
    let added = false
    onUpdateModelAliases((current) => {
      const existing = current[source] ?? []
      if (existing.includes(target)) return current
      added = true
      return { ...current, [source]: [...existing, target] }
    })
    if (added) {
      setAliasTarget('')
    }
  }

  function updateAliasSource(previousSource: string, nextSource: string) {
    const trimmed = nextSource.trim()
    if (!trimmed || trimmed === previousSource) return
    onUpdateModelAliases((current) => {
      const targets = current[previousSource]
      if (!targets?.length) return current
      if (trimmed === previousSource) return current
      const nextAliases = { ...current }
      delete nextAliases[previousSource]
      const merged = [...(nextAliases[trimmed] ?? [])]
      for (const target of targets) {
        if (target !== trimmed && !merged.includes(target)) merged.push(target)
      }
      if (!merged.length) {
        delete nextAliases[trimmed]
      } else {
        nextAliases[trimmed] = merged
      }
      return nextAliases
    })
  }

  function addAliasTarget(source: string, target: string) {
    const trimmed = target.trim()
    if (!source || !trimmed || source === trimmed) return
    onUpdateModelAliases((current) => {
      const existing = current[source] ?? []
      if (existing.includes(trimmed)) return current
      return { ...current, [source]: [...existing, trimmed] }
    })
  }

  function removeAliasTarget(source: string, target: string) {
    onUpdateModelAliases((current) => {
      const existing = current[source] ?? []
      const nextTargets = existing.filter((item) => item !== target)
      const nextAliases = { ...current }
      if (!nextTargets.length) delete nextAliases[source]
      else nextAliases[source] = nextTargets
      return nextAliases
    })
  }

  function removeAlias(source: string) {
    onUpdateModelAliases((current) => {
      const nextAliases = { ...current }
      delete nextAliases[source]
      return nextAliases
    })
  }

  function moveAliasTarget(source: string, index: number, action: ListMoveAction) {
    onUpdateModelAliases((current) => {
      const existing = current[source]
      if (!existing?.length) return current
      const nextTargets = moveItemByAction(existing, index, action)
      if (!nextTargets) return current
      return { ...current, [source]: nextTargets }
    })
  }

  function reorderAliasTarget(source: string, fromIndex: number, toIndex: number) {
    onUpdateModelAliases((current) => {
      const existing = current[source]
      if (!existing?.length) return current
      const nextTargets = reorderItem(existing, fromIndex, toIndex)
      if (!nextTargets) return current
      return { ...current, [source]: nextTargets }
    })
  }

  return (
    <>
      <section className="page-intro">
        <div>
          <span className="eyebrow">{t('routing.eyebrow')}</span>
          <h2>{t('routing.title')}</h2>
          <p>{t('routing.desc')}</p>
        </div>
        <div className="route-summary"><span className="status-dot" />{t('routing.autoRouting')}</div>
      </section>
      <div className="content-grid routing-grid">
        <section className="panel priority-panel">
          <div className="panel-heading">
            <div><span className="eyebrow">{t('routing.priorityEyebrow')}</span><h3>{t('routing.priorityTitle')}</h3></div>
            <span className="panel-caption">{t('routing.priorityCaption')}</span>
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
                  <button className="priority-drag-handle" type="button" title={t('routing.dragOrder', { name: meta.label })} aria-label={t('routing.dragOrder', { name: meta.label })}>
                    <Icon name="grip" size={16} />
                  </button>
                  <span className="priority-number">{String(index + 1).padStart(2, '0')}</span>
                  <div className={`provider-avatar ${kind === 'grok' ? 'grok-avatar' : ''}`} style={{ backgroundColor: meta.color }}>{providerMarkText(kind)}</div>
                  <div className="priority-copy"><strong>{meta.label}</strong><span>{count ? t('routing.activeConfigs', { count }) : t('routing.noActiveConfigs')}</span></div>
                  <div className="priority-actions">
                    <button className="icon-button subtle" type="button" title={t('routing.toTop')} disabled={index === 0} onClick={() => onMove(index, 'top')}><Icon name="toTop" size={15} /></button>
                    <button className="icon-button subtle" type="button" title={t('routing.moveUp')} disabled={index === 0} onClick={() => onMove(index, -1)}><Icon name="arrowUp" size={15} /></button>
                    <button className="icon-button subtle" type="button" title={t('routing.moveDown')} disabled={index === priority.length - 1} onClick={() => onMove(index, 1)}><Icon name="arrowDown" size={15} /></button>
                    <button className="icon-button subtle" type="button" title={t('routing.toBottom')} disabled={index === priority.length - 1} onClick={() => onMove(index, 'bottom')}><Icon name="toBottom" size={15} /></button>
                  </div>
                </div>
              )
            })}
          </div>
          <div className="info-callout"><Icon name="pulse" size={16} /><span>{t('routing.priorityHint')}</span></div>
        </section>
        <section className="panel fallback-panel">
          <div className="panel-heading">
            <div><span className="eyebrow">{t('routing.fallbackEyebrow')}</span><h3>{t('routing.fallbackTitle')}</h3></div>
            <button className="button button-secondary fallback-add-button" type="button" onClick={onAddFallback}><Icon name="plus" size={16} />{t('routing.addModel')}</button>
          </div>
          <p className="panel-description">{t('routing.fallbackDesc')}</p>
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
                <button className="priority-drag-handle" type="button" title={t('routing.dragOrder', { name: model })} aria-label={t('routing.dragOrder', { name: model })}>
                  <Icon name="grip" size={15} />
                </button>
                <span className="fallback-index">{index + 1}</span>
                <div className="fallback-copy"><strong>{model}</strong><span>{allModels.includes(model) ? t('routing.registeredModel') : t('routing.pendingProvider')}</span></div>
                <div className="priority-actions">
                  <button className="icon-button subtle" type="button" title={t('routing.toTop')} disabled={index === 0} onClick={() => onMoveFallback(index, 'top')}><Icon name="toTop" size={15} /></button>
                  <button className="icon-button subtle" type="button" title={t('routing.moveUp')} disabled={index === 0} onClick={() => onMoveFallback(index, -1)}><Icon name="arrowUp" size={15} /></button>
                  <button className="icon-button subtle" type="button" title={t('routing.moveDown')} disabled={index === fallbacks.length - 1} onClick={() => onMoveFallback(index, 1)}><Icon name="arrowDown" size={15} /></button>
                  <button className="icon-button subtle" type="button" title={t('routing.toBottom')} disabled={index === fallbacks.length - 1} onClick={() => onMoveFallback(index, 'bottom')}><Icon name="toBottom" size={15} /></button>
                  <button className="icon-button subtle danger-button" type="button" title={t('routing.removeFallback')} onClick={() => onRemoveFallback(model)}><Icon name="trash" size={15} /></button>
                </div>
              </div>
            ))}
          </div>
          {!fallbacks.length && <div className="empty-state small"><span>{t('routing.noFallbacks')}</span></div>}
          <div className="model-catalog">
            <span className="section-label">{t('routing.availableModels')}</span>
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
              <div className="empty-state small"><span>{t('routing.noSyncedModels')}</span></div>
            )}
          </div>
        </section>
        <section className="panel model-alias-panel">
          <div className="panel-heading">
            <div><span className="eyebrow">{t('routing.aliasEyebrow')}</span><h3>{t('routing.aliasTitle')}</h3></div>
            <span className="panel-caption">{t('routing.aliasCaption', { groups: aliasRows.length, targets: aliasTargetCount })}</span>
          </div>
          <p className="panel-description">
            {t('routing.aliasDesc')}
          </p>
          <div className="alias-add-row">
            <div className="alias-add-header">
              <div className="alias-add-title">
                <span className="alias-add-label">{t('routing.newAlias')}</span>
                <span className="alias-add-hint">{t('routing.aliasHint')}</span>
              </div>
            </div>
            <div className="alias-add-fields">
              <label className="alias-add-field">
                <span className="alias-field-label">{t('routing.primaryLabel')}</span>
                <SelectControl
                  mono
                  searchable
                  searchPlaceholder={t('routing.filterPrimary')}
                  value={aliasSource}
                  options={configuredModelOptions.length ? aliasSourceOptions : [{ value: '', label: t('routing.noModelsAvailable') }]}
                  onChange={setAliasSource}
                  disabled={!configuredModelOptions.length}
                  ariaLabel={t('routing.selectPrimary')}
                />
              </label>
              <span className="alias-add-arrow" aria-hidden="true">
                <span className="alias-add-arrow-badge">→</span>
              </span>
              <label className="alias-add-field">
                <span className="alias-field-label">{t('routing.fallbackLabel')}</span>
                <SelectControl
                  mono
                  searchable
                  searchPlaceholder={t('routing.filterFallback')}
                  value={aliasTarget}
                  options={configuredModelOptions.length ? aliasTargetOptions : [{ value: '', label: t('routing.noModelsAvailable') }]}
                  onChange={setAliasTarget}
                  disabled={!configuredModelOptions.length || !aliasSource}
                  ariaLabel={t('routing.selectFallback')}
                />
              </label>
              <button
                className={`button alias-add-button${canAddAlias ? ' alias-add-button-ready' : ''}`}
                type="button"
                title={
                  !aliasSource || !aliasTarget
                    ? t('routing.needBothModels')
                    : aliasSource === aliasTarget
                      ? t('routing.sameModel')
                      : existingTargets.includes(aliasTarget)
                        ? t('routing.alreadyAdded')
                        : t('routing.addAliasTarget')
                }
                disabled={!canAddAlias}
                onClick={addAlias}
              >
                <Icon name="plus" size={15} />
                {t('routing.addTarget')}
              </button>
            </div>
          </div>
          <div className="alias-list">
            {aliasRows.map(([source, targets]) => (
              <div className="alias-row" key={source}>
                <span className="alias-field-label alias-source-label">{t('routing.primary')}</span>
                <div className="alias-targets-heading">
                  <span className="alias-field-label">{t('routing.fallbackAfter')}</span>
                  <span className="alias-target-count">{t('routing.targetCount', { count: targets.length })}</span>
                </div>
                <span className="alias-row-spacer" aria-hidden="true" />
                <div className="alias-source-control">
                  <SelectControl
                    mono
                    searchable
                    searchPlaceholder={t('routing.filterPrimary')}
                    value={source}
                    options={configuredModelOptions.length ? configuredModelOptions : [{ value: '', label: t('routing.noModelsAvailable') }]}
                    onChange={(nextSource) => updateAliasSource(source, nextSource)}
                    disabled={!configuredModelOptions.length}
                    ariaLabel={t('routing.editPrimary', { source })}
                  />
                </div>
                <span className="alias-arrow" aria-hidden="true">→</span>
                <div className="alias-targets">
                  {targets.map((target, index) => (
                    <span
                      className={`alias-target-chip ${draggedAliasTarget?.source === source && draggedAliasTarget.index === index ? 'is-dragging' : ''}`}
                      key={`${source}:${target}`}
                      draggable={targets.length > 1}
                      title={targets.length > 1 ? t('routing.dragFallbackOrder') : undefined}
                      onDragStart={(event) => {
                        if (targets.length <= 1) {
                          event.preventDefault()
                          return
                        }
                        setDraggedAliasTarget({ source, index })
                        event.dataTransfer.effectAllowed = 'move'
                        event.dataTransfer.setData('text/plain', `${source}:${target}`)
                      }}
                      onDragOver={(event) => {
                        if (draggedAliasTarget?.source !== source) return
                        event.preventDefault()
                        event.dataTransfer.dropEffect = 'move'
                      }}
                      onDrop={(event) => {
                        event.preventDefault()
                        if (draggedAliasTarget?.source === source) {
                          reorderAliasTarget(source, draggedAliasTarget.index, index)
                        }
                        setDraggedAliasTarget(null)
                      }}
                      onDragEnd={() => setDraggedAliasTarget(null)}
                    >
                      {targets.length > 1 && (
                        <span className="alias-chip-grip" aria-hidden="true">
                          <Icon name="grip" size={12} />
                        </span>
                      )}
                      <span className="alias-target-index">{index + 1}</span>
                      <code>{target}</code>
                      {targets.length > 1 && (
                        <span className="alias-chip-actions">
                          <button
                            className="alias-chip-move"
                            type="button"
                            title={t('routing.moveUp')}
                            disabled={index === 0}
                            onClick={() => moveAliasTarget(source, index, -1)}
                            onPointerDown={(event) => event.stopPropagation()}
                          >
                            <Icon name="arrowUp" size={12} />
                          </button>
                          <button
                            className="alias-chip-move"
                            type="button"
                            title={t('routing.moveDown')}
                            disabled={index === targets.length - 1}
                            onClick={() => moveAliasTarget(source, index, 1)}
                            onPointerDown={(event) => event.stopPropagation()}
                          >
                            <Icon name="arrowDown" size={12} />
                          </button>
                        </span>
                      )}
                      <button
                        className="alias-chip-remove"
                        type="button"
                        title={t('routing.removeTarget', { target })}
                        onClick={() => removeAliasTarget(source, target)}
                        onPointerDown={(event) => event.stopPropagation()}
                      >
                        <Icon name="close" size={12} />
                      </button>
                    </span>
                  ))}
                  <div className="alias-target-add">
                    <SelectControl
                      mono
                      searchable
                      searchPlaceholder={t('routing.filterFallback')}
                      value=""
                      options={[
                        { value: '', label: t('routing.addTargetShort') },
                        ...configuredModelOptions.filter(
                          (option) => option.value !== source && !targets.includes(option.value),
                        ),
                      ]}
                      onChange={(nextTarget) => addAliasTarget(source, nextTarget)}
                      disabled={!configuredModelOptions.length}
                      ariaLabel={t('routing.addFallbackFor', { source })}
                    />
                  </div>
                </div>
                <button
                  className="icon-button subtle danger-button alias-row-remove"
                  type="button"
                  title={t('routing.removeAliasGroup')}
                  onClick={() => removeAlias(source)}
                >
                  <Icon name="trash" size={15} />
                </button>
              </div>
            ))}
            {!aliasRows.length && <div className="empty-state small"><span>{t('routing.noAliases')}</span></div>}
          </div>
        </section>
      </div>
    </>
  )
}
