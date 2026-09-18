import { useMemo, useState } from 'react'
import { Icon } from '../../components/Icon'
import { useI18n } from '../../lib/i18n'

export function FallbackModelModal({
  allModels,
  fallbacks,
  onClose,
  onSubmit,
}: {
  allModels: string[]
  fallbacks: string[]
  onClose: () => void
  onSubmit: (models: string[]) => void
}) {
  const { t } = useI18n()
  const [manualInput, setManualInput] = useState('')
  const [modelSearch, setModelSearch] = useState('')
  const [showSelectedModelsOnly, setShowSelectedModelsOnly] = useState(false)
  const [selectedModels, setSelectedModels] = useState<string[]>([])
  const fallbackSet = useMemo(() => new Set(fallbacks), [fallbacks])
  const selectedModelSet = useMemo(() => new Set(selectedModels), [selectedModels])
  const modelOptionList = useMemo(
    () => Array.from(new Set([...allModels.filter((model) => !fallbackSet.has(model)), ...selectedModels])).sort(),
    [allModels, fallbackSet, selectedModels],
  )
  const normalizedModelSearch = modelSearch.trim().toLowerCase()
  const filteredModelOptions = modelOptionList.filter((model) =>
    model.toLowerCase().includes(normalizedModelSearch) &&
    (!showSelectedModelsOnly || selectedModelSet.has(model)),
  )
  const visibleSelectedCount = filteredModelOptions.filter((model) => selectedModelSet.has(model)).length

  function setModelChecked(model: string, checked: boolean) {
    if (checked) {
      if (selectedModelSet.has(model) || fallbackSet.has(model)) return
      setSelectedModels([...selectedModels, model])
      return
    }
    setSelectedModels(selectedModels.filter((item) => item !== model))
  }

  function setVisibleModelsChecked(checked: boolean) {
    if (checked) {
      const nextModels = filteredModelOptions.filter((model) => !selectedModelSet.has(model) && !fallbackSet.has(model))
      if (!nextModels.length) return
      setSelectedModels([...selectedModels, ...nextModels])
      return
    }
    const visibleModels = new Set(filteredModelOptions)
    setSelectedModels(selectedModels.filter((model) => !visibleModels.has(model)))
  }

  function addManualModel() {
    const model = manualInput.trim()
    if (!model || fallbackSet.has(model) || selectedModelSet.has(model)) return
    setSelectedModels([...selectedModels, model])
    setManualInput('')
  }

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const manualModel = manualInput.trim()
    const models =
      manualModel && !fallbackSet.has(manualModel) && !selectedModelSet.has(manualModel)
        ? [...selectedModels, manualModel]
        : selectedModels
    onSubmit(models)
  }

  return (
    <div className="llm-modal-backdrop" onMouseDown={onClose}>
      <form className="modal fallback-picker-modal" onSubmit={submit} onMouseDown={(event) => event.stopPropagation()}>
        <div className="modal-heading">
          <div>
            <span className="eyebrow">{t('fallback.eyebrow')}</span>
            <h2>{t('fallback.title')}</h2>
          </div>
          <button className="icon-button" type="button" title={t('fallback.close')} onClick={onClose}>
            ×
          </button>
        </div>
        <div className="model-editor">
          <div className="model-add-row">
            <input
              autoFocus
              value={manualInput}
              onChange={(event) => setManualInput(event.target.value)}
              onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); addManualModel() } }}
              placeholder={t('fallback.inputPlaceholder')}
            />
            <button className="icon-button accent-button" type="button" title={t('fallback.addToPending')} onClick={addManualModel}>
              <Icon name="plus" size={16} />
            </button>
          </div>
          <div className="model-option-panel">
            <div className="model-option-toolbar">
              <div>
                <strong>{t('fallback.summary', { selected: selectedModels.length, available: modelOptionList.length })}</strong>
                <span>{t('fallback.visibleSummary', { visible: filteredModelOptions.length, selected: visibleSelectedCount })}</span>
              </div>
              <label className="model-search-field">
                <Icon name="search" size={14} />
                <input
                  value={modelSearch}
                  onChange={(event) => setModelSearch(event.target.value)}
                  placeholder={t('fallback.searchPlaceholder')}
                />
              </label>
              <div className="model-option-actions">
                <button className="text-button" type="button" onClick={() => setVisibleModelsChecked(true)}>
                  {t('fallback.selectAll')}
                </button>
                <button className="text-button" type="button" onClick={() => setVisibleModelsChecked(false)}>
                  {t('fallback.deselectAll')}
                </button>
                <button className="text-button danger-text" type="button" onClick={() => setSelectedModels([])}>
                  {t('fallback.clearSelected')}
                </button>
              </div>
            </div>
            <label className="model-selected-filter">
              <input
                type="checkbox"
                checked={showSelectedModelsOnly}
                onChange={(event) => setShowSelectedModelsOnly(event.target.checked)}
              />
              <span>{t('fallback.showSelectedOnly')}</span>
            </label>
            <div className="model-option-list selectable fallback-picker-list">
              {filteredModelOptions.map((model) => {
                const checked = selectedModelSet.has(model)
                return (
                  <label className={`model-check-option ${checked ? 'selected' : ''}`} key={model}>
                    <input
                      type="checkbox"
                      checked={checked}
                      onChange={(event) => setModelChecked(model, event.target.checked)}
                    />
                    <span>{model}</span>
                  </label>
                )
              })}
              {!filteredModelOptions.length && (
                <span className="model-sync-status muted-copy">{t('fallback.noMatch')}</span>
              )}
            </div>
          </div>
        </div>
        <div className="modal-actions">
          <button className="button button-secondary" type="button" onClick={onClose}>
            {t('fallback.cancel')}
          </button>
          <button className="button button-primary" type="submit">
            <Icon name="plus" size={16} />
            {t('fallback.addSelected')}
          </button>
        </div>
      </form>
    </div>
  )
}
