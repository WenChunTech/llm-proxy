import { useMemo, useState } from 'react'
import { Icon } from '../../components/Icon'

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
    <div className="modal-backdrop" onMouseDown={onClose}>
      <form className="modal fallback-picker-modal" onSubmit={submit} onMouseDown={(event) => event.stopPropagation()}>
        <div className="modal-heading">
          <div>
            <span className="eyebrow">FALLBACK CHAIN</span>
            <h2>添加备用模型</h2>
          </div>
          <button className="icon-button" type="button" title="关闭" onClick={onClose}>
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
              placeholder="输入模型名称"
            />
            <button className="icon-button accent-button" type="button" title="加入待添加列表" onClick={addManualModel}>
              <Icon name="plus" size={16} />
            </button>
          </div>
          <div className="model-option-panel">
            <div className="model-option-toolbar">
              <div>
                <strong>待添加 {selectedModels.length} 个 / 可选 {modelOptionList.length} 个</strong>
                <span>{filteredModelOptions.length} 个当前可见，{visibleSelectedCount} 个已选</span>
              </div>
              <label className="model-search-field">
                <Icon name="search" size={14} />
                <input
                  value={modelSearch}
                  onChange={(event) => setModelSearch(event.target.value)}
                  placeholder="搜索模型"
                />
              </label>
              <div className="model-option-actions">
                <button className="text-button" type="button" onClick={() => setVisibleModelsChecked(true)}>
                  全选当前
                </button>
                <button className="text-button" type="button" onClick={() => setVisibleModelsChecked(false)}>
                  取消选择
                </button>
                <button className="text-button danger-text" type="button" onClick={() => setSelectedModels([])}>
                  清空已选
                </button>
              </div>
            </div>
            <label className="model-selected-filter">
              <input
                type="checkbox"
                checked={showSelectedModelsOnly}
                onChange={(event) => setShowSelectedModelsOnly(event.target.checked)}
              />
              <span>仅显示已选模型</span>
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
                <span className="model-sync-status muted-copy">没有匹配的模型</span>
              )}
            </div>
          </div>
        </div>
        <div className="modal-actions">
          <button className="button button-secondary" type="button" onClick={onClose}>
            取消
          </button>
          <button className="button button-primary" type="submit">
            <Icon name="plus" size={16} />
            添加所选模型
          </button>
        </div>
      </form>
    </div>
  )
}
