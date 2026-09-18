import { useCallback, useEffect, useMemo, useState } from 'react'
import { Icon } from '../../components/Icon'
import { SelectControl } from '../../components/controls/SelectControl'
import {
  defaultBaseUrlForNewProvider,
  editableNewKinds,
  effectiveBaseUrlForProvider,
  providerMeta,
} from '../../config/providers'
import { apiAuthHeaders } from '../../lib/api'
import { stringifyAuth } from '../../lib/authValues'
import { copyText, readResponseStream } from '../../lib/browser'
import { toApiProviderDraft } from '../../lib/configImportExport'
import { readJsonFiles } from '../../lib/fileImport'
import {
  buildProviderTestCurl,
  getProviderModelsEndpointFor,
  hasUsableProviderConnection,
  normalizeProviderModelEntries,
} from '../../lib/providerRequests'
import { isRecord } from '../../lib/records'
import type {
  ProviderDraft,
  ProviderKind,
  ProviderModelsPayload,
  ProviderTestPayload,
} from '../../types/domain'
import { useI18n } from '../../lib/i18n'


function stringifyHeaders(headers: Record<string, string>) {
  return Object.keys(headers).length ? JSON.stringify(headers, null, 2) : ''
}

export function ProviderModal({
  provider,
  isEditing,
  allowKindChange,
  accessKey,
  onChange,
  onClose,
  onSubmit,
}: {
  provider: ProviderDraft
  isEditing: boolean
  allowKindChange: boolean
  accessKey: string
  onChange: (provider: ProviderDraft) => void
  onClose: () => void
  onSubmit: (event: React.FormEvent<HTMLFormElement>) => void
}) {
  const { t } = useI18n()
  const [modelInput, setModelInput] = useState('')
  const [modelSearch, setModelSearch] = useState('')
  const [showSelectedModelsOnly, setShowSelectedModelsOnly] = useState(false)
  const [headersJson, setHeadersJson] = useState(() => stringifyHeaders(provider.headers))
  const [headersError, setHeadersError] = useState('')
  const [authJson, setAuthJson] = useState(() => stringifyAuth(provider.auth))
  const [authError, setAuthError] = useState('')
  const [testModel, setTestModel] = useState(() => provider.models[0] ?? '')
  const [testPrompt, setTestPrompt] = useState('hello')
  const [testStream, setTestStream] = useState(true)
  const [testStatus, setTestStatus] = useState<'idle' | 'loading' | 'ok' | 'error'>('idle')
  const [testMessage, setTestMessage] = useState('')
  const [testRawData, setTestRawData] = useState('')
  const [curlCopyStatus, setCurlCopyStatus] = useState('')
  const [modelEndpoint, setModelEndpoint] = useState('')
  const [modelOptions, setModelOptions] = useState<string[]>([])
  const [modelStatus, setModelStatus] = useState<'waiting' | 'loading' | 'ready' | 'error'>(
    hasUsableProviderConnection(provider) ? 'loading' : 'waiting',
  )
  const providerKind = provider.kind
  const providerEffectiveBaseUrl = effectiveBaseUrlForProvider(provider)
  const providerApiKey = provider.apiKey
  const providerAuth = provider.auth
  const supportsAuthJson = providerKind === 'codex' || providerKind === 'grok'
  const needsAuthJson = supportsAuthJson && !providerApiKey.trim()
  const selectedModelSet = useMemo(() => new Set(provider.models), [provider.models])
  const modelOptionList = useMemo(
    () => Array.from(new Set([...modelOptions, ...provider.models])).filter(Boolean).sort(),
    [modelOptions, provider.models],
  )
  const testModelOptions = useMemo(
    () => provider.models.filter(Boolean).sort(),
    [provider.models],
  )
  const normalizedModelSearch = modelSearch.trim().toLowerCase()
  const filteredModelOptions = modelOptionList.filter((model) =>
    model.toLowerCase().includes(normalizedModelSearch) &&
    (!showSelectedModelsOnly || selectedModelSet.has(model)),
  )
  const visibleSelectedCount = filteredModelOptions.filter((model) => selectedModelSet.has(model)).length
  const canSaveProvider =
    !headersError && !authError && (!needsAuthJson || Boolean(provider.auth))

  const syncProviderModels = useCallback(async (signal?: AbortSignal) => {
    const endpoint = getProviderModelsEndpointFor(providerKind, providerEffectiveBaseUrl)
    if (!endpoint) {
      setModelOptions([])
      setModelEndpoint('')
      setModelStatus('waiting')
      return
    }
    setModelEndpoint(endpoint)
    setModelStatus('loading')

    try {
      const response = await fetch('/api/provider-models', {
        method: 'POST',
        headers: apiAuthHeaders(accessKey, { 'content-type': 'application/json' }),
        body: JSON.stringify({
          kind: providerKind,
          base_url: providerEffectiveBaseUrl,
          api_key: providerApiKey.trim(),
          headers: provider.headers,
          auth: providerAuth,
        }),
        signal,
      })
      if (!response.ok) throw new Error(await response.text())
      const payload = (await response.json()) as ProviderModelsPayload
      setModelEndpoint(payload.endpoint ?? endpoint)
      setModelOptions(normalizeProviderModelEntries(payload.data))
      setModelStatus('ready')
    } catch (error) {
      if (signal?.aborted || (error instanceof DOMException && error.name === 'AbortError')) return
      setModelOptions([])
      setModelStatus('error')
    }
  }, [accessKey, provider.headers, providerApiKey, providerAuth, providerEffectiveBaseUrl, providerKind])

  useEffect(() => {
    const endpoint = getProviderModelsEndpointFor(providerKind, providerEffectiveBaseUrl)
    if (!endpoint) {
      setModelOptions([])
      setModelEndpoint('')
      setModelStatus('waiting')
      return
    }
    setModelEndpoint(endpoint)

    const controller = new AbortController()
    const timer = window.setTimeout(() => {
      void syncProviderModels(controller.signal)
    }, 450)

    return () => {
      window.clearTimeout(timer)
      controller.abort()
    }
  }, [providerEffectiveBaseUrl, providerKind, syncProviderModels])

  useEffect(() => {
    setTestModel((current) => current && provider.models.includes(current) ? current : provider.models[0] ?? '')
    setTestStatus('idle')
    setTestMessage('')
    setTestRawData('')
  }, [provider.models])

  useEffect(() => {
    if (!testModel && testModelOptions[0]) setTestModel(testModelOptions[0])
  }, [testModel, testModelOptions])

  function addModel() {
    const model = modelInput.trim()
    if (!model || selectedModelSet.has(model)) return
    onChange({ ...provider, models: [...provider.models, model] })
    if (!testModel) setTestModel(model)
    setModelInput('')
  }

  function setModelChecked(model: string, checked: boolean) {
    if (checked) {
      if (selectedModelSet.has(model)) return
      onChange({ ...provider, models: [...provider.models, model] })
      return
    }
    removeModel(model)
  }

  function setVisibleModelsChecked(checked: boolean) {
    if (checked) {
      const nextModels = filteredModelOptions.filter((model) => !selectedModelSet.has(model))
      if (!nextModels.length) return
      onChange({ ...provider, models: [...provider.models, ...nextModels] })
      return
    }
    const visibleModels = new Set(filteredModelOptions)
    onChange({ ...provider, models: provider.models.filter((model) => !visibleModels.has(model)) })
  }

  function clearSelectedModels() {
    if (!provider.models.length) return
    onChange({ ...provider, models: [] })
  }

  function removeModel(model: string) {
    onChange({ ...provider, models: provider.models.filter((item) => item !== model) })
  }

  function updateHeadersJson(value: string) {
    setHeadersJson(value)
    const trimmed = value.trim()
    if (!trimmed) {
      onChange({ ...provider, headers: {} })
      setHeadersError('')
      return
    }
    try {
      const parsed = JSON.parse(trimmed)
      if (!isRecord(parsed)) {
        setHeadersError(t('modal.headersMustBeObject'))
        return
      }
      const nextHeaders: Record<string, string> = {}
      for (const [name, headerValue] of Object.entries(parsed)) {
        const key = name.trim()
        if (!key) {
          setHeadersError(t('modal.headerNameEmpty'))
          return
        }
        if (typeof headerValue === 'string') {
          nextHeaders[key] = headerValue
          continue
        }
        if (typeof headerValue === 'number' || typeof headerValue === 'boolean') {
          nextHeaders[key] = String(headerValue)
          continue
        }
        setHeadersError(t('modal.headerValueString'))
        return
      }
      onChange({ ...provider, headers: nextHeaders })
      setHeadersError('')
    } catch {
      setHeadersError(t('modal.jsonInvalid'))
    }
  }

  function changeKind(kind: ProviderKind) {
    onChange({
      ...provider,
      kind,
      baseUrl: provider.baseUrl && provider.baseUrl !== 'https://' ? provider.baseUrl : defaultBaseUrlForNewProvider(kind),
      auth: kind === 'codex' || kind === 'grok' ? provider.auth : undefined,
    })
    if (kind !== 'codex' && kind !== 'grok') {
      setAuthJson('')
      setAuthError('')
    }
  }

  function updateAuthJson(value: string) {
    setAuthJson(value)
    const trimmed = value.trim()
    if (!trimmed) {
      onChange({ ...provider, auth: undefined })
      setAuthError(needsAuthJson ? t('modal.authEmpty') : '')
      return
    }
    try {
      const parsed = JSON.parse(trimmed)
      if (!Array.isArray(parsed) && !isRecord(parsed)) {
        setAuthError(t('modal.authMustBeObjectOrArray'))
        return
      }
      onChange({
        ...provider,
        auth: parsed,
      })
      setAuthError('')
    } catch {
      setAuthError(t('modal.jsonInvalid'))
    }
  }

  async function importAuthFiles(files: FileList | null) {
    if (!files?.length) return
    try {
      const values = await readJsonFiles(files)
      if (!values.every((value) => isRecord(value))) {
        setAuthError(t('modal.authMustBeObjectOrArrayPlural'))
        return
      }
      const nextAuth = values.length === 1 ? values[0] : values
      const text = JSON.stringify(nextAuth, null, 2)
      setAuthJson(text)
      onChange({
        ...provider,
        auth: nextAuth,
      })
      setAuthError('')
    } catch {
      setAuthError(t('modal.authFileReadFailed'))
    }
  }

  async function testCurrentModel() {
    const selected = testModel.trim()
    const model = selected && provider.models.includes(selected) ? selected : provider.models[0]
    if (!model) {
      setTestStatus('error')
      setTestMessage(t('modal.selectOrAddModel'))
      return
    }
    setTestStatus('loading')
    setTestMessage('')
    setTestRawData('')
    try {
      const response = await fetch('/api/provider-test', {
        method: 'POST',
        headers: apiAuthHeaders(accessKey, { 'content-type': 'application/json' }),
        body: JSON.stringify({
          provider: toApiProviderDraft(provider),
          model,
          prompt: testPrompt.trim() || 'hello',
          stream: testStream,
        }),
      })
      if (testStream) {
        const rawBody = await readResponseStream(response, setTestRawData)
        setTestStatus(response.ok ? 'ok' : 'error')
        setTestMessage(response.ok ? t('modal.detectPassed', { status: response.status }) : t('modal.detectFailed', { status: response.status }))
        setTestRawData(rawBody)
        return
      }
      const payload = (await response.json()) as ProviderTestPayload | { error?: { message?: string } }
      if (!response.ok) throw new Error('error' in payload ? payload.error?.message : '')
      const result = payload as ProviderTestPayload
      setTestStatus(result.ok ? 'ok' : 'error')
      setTestMessage(result.ok ? t('modal.detectPassed', { status: result.status }) : t('modal.detectFailed', { status: result.status }))
      setTestRawData(result.raw_body || result.body_preview || '')
    } catch (error) {
      setTestStatus('error')
      setTestMessage(error instanceof Error && error.message ? error.message : t('modal.detectFailedShort'))
      setTestRawData('')
    }
  }

  async function copyTestCurl() {
    const selected = testModel.trim()
    const model = selected && provider.models.includes(selected) ? selected : provider.models[0]
    if (!model) {
      setCurlCopyStatus(t('modal.selectOrAddModel'))
      return
    }
    const command = buildProviderTestCurl(provider, model, testPrompt.trim() || 'hello', testStream)
    try {
      await copyText(command)
      setCurlCopyStatus(t('modal.curlCopied'))
      window.setTimeout(() => setCurlCopyStatus(''), 1800)
    } catch {
      setCurlCopyStatus(t('modal.copyFailed'))
    }
  }

  return (
    <div className="llm-modal-backdrop" onMouseDown={onClose}>
      <form className="modal provider-modal" onSubmit={onSubmit} onMouseDown={(event) => event.stopPropagation()}>
        <div className="modal-heading"><div><span className="eyebrow">{t('modal.eyebrow')}</span><h2>{isEditing ? t('modal.editTitle') : t('modal.addTitle')}</h2></div><button className="icon-button" type="button" title={t('modal.close')} onClick={onClose}>×</button></div>
        <div className="provider-modal-layout">
          <section className="config-section">
            <div className="config-section-heading">
              <span className="eyebrow">{t('modal.connectionEyebrow')}</span>
              <h3>{t('modal.connectionTitle')}</h3>
            </div>
            <div className="form-grid">
              <label className="field"><span>{t('modal.displayName')}</span><input required value={provider.name} onChange={(event) => onChange({ ...provider, name: event.target.value })} placeholder={t("modal.displayNamePlaceholder")} /></label>
              <div className="field">
                <span>{t('modal.providerType')}</span>
                <SelectControl
                  value={provider.kind}
                  options={editableNewKinds.map((kind) => ({
                    value: kind,
                    label: providerMeta[kind].label,
                    accent: providerMeta[kind].color,
                  }))}
                  onChange={changeKind}
                  disabled={!allowKindChange}
                  ariaLabel={t("modal.selectProviderType")}
                />
              </div>
            </div>
            <label className="field">
              <span>{t('modal.baseUrl')}</span>
              <input
                required={!supportsAuthJson}
                value={provider.baseUrl}
                onChange={(event) => onChange({ ...provider, baseUrl: event.target.value })}
                placeholder={supportsAuthJson ? t("modal.baseUrlAuthPlaceholder") : t("modal.baseUrlPlaceholder")}
              />
              {supportsAuthJson && (
                <small>{t('modal.currentEndpoint', { url: providerEffectiveBaseUrl })}</small>
              )}
            </label>
            <label className="field"><span>{t('modal.apiKey')}</span><input type="password" autoComplete="current-password" value={provider.apiKey} onChange={(event) => onChange({ ...provider, apiKey: event.target.value })} placeholder={t('modal.apiKeyPlaceholder')} /></label>
          </section>
          <section className="config-section">
            <div className="config-section-heading">
              <span className="eyebrow">{t('modal.headersEyebrow')}</span>
              <h3>{t('modal.headersTitle')}</h3>
            </div>
            <div className="header-editor">
              <textarea
                value={headersJson}
                onChange={(event) => updateHeadersJson(event.target.value)}
                placeholder='{"X-Custom-Header":"value","User-Agent":"my-client"}'
                spellCheck={false}
              />
              {headersError ? (
                <span className="model-sync-status muted-copy">{headersError}</span>
              ) : (
                <span className="model-sync-status muted-copy">
                  {supportsAuthJson
                    ? t('modal.headersHintAuth')
                    : t('modal.headersHint')}
                </span>
              )}
            </div>
          </section>
          {supportsAuthJson && (
            <section className="config-section">
              <div className="config-section-heading">
                <span className="eyebrow">AUTH</span>
                <h3>{t('modal.authTitle')}</h3>
              </div>
              <div className="auth-editor">
                <div className="auth-actions">
                  <label className="button button-secondary import-button compact">
                    <Icon name="upload" size={15} />
                    {t('modal.jsonFile')}
                    <input
                      type="file"
                      accept="application/json,.json"
                      onChange={(event) => {
                        void importAuthFiles(event.target.files)
                        event.currentTarget.value = ''
                      }}
                    />
                  </label>
                  <label className="button button-secondary import-button compact">
                    <Icon name="upload" size={15} />
                    {t('modal.jsonDir')}
                    <input
                      type="file"
                      accept="application/json,.json"
                      multiple
                      {...{ webkitdirectory: '' }}
                      onChange={(event) => {
                        void importAuthFiles(event.target.files)
                        event.currentTarget.value = ''
                      }}
                    />
                  </label>
                </div>
                <textarea
                  value={authJson}
                  onChange={(event) => updateAuthJson(event.target.value)}
                  placeholder={t('modal.authPlaceholder')}
                />
                {authError ? (
                  <span className="model-sync-status muted-copy">{authError}</span>
                ) : (
                  <span className="model-sync-status muted-copy">
                    t('modal.authHint')
                  </span>
                )}
              </div>
            </section>
          )}
          <section className="config-section">
            <div className="config-section-heading">
              <div>
                <span className="eyebrow">{t('modal.modelsEyebrow')}</span>
                <h3>{t('modal.modelsTitle')}</h3>
              </div>
              <button
                className="button button-secondary compact-model-sync-button"
                type="button"
                disabled={!modelEndpoint || modelStatus === 'loading'}
                onClick={() => void syncProviderModels()}
              >
                <Icon name="download" size={15} />
                {modelStatus === 'loading' ? t('modal.fetching') : t('modal.fetchModels')}
              </button>
            </div>
            <div className="model-editor">
              <div className="model-add-row">
                <input
                  value={modelInput}
                  onChange={(event) => setModelInput(event.target.value)}
                  onKeyDown={(event) => { if (event.key === 'Enter') { event.preventDefault(); addModel() } }}
                  placeholder={t("modal.modelPlaceholder")}
                />
                <button className="icon-button accent-button" type="button" title={t("modal.addModel")} onClick={addModel}><Icon name="plus" size={16} /></button>
              </div>
              {modelEndpoint && (
                <div className="model-endpoint-row">
                  <span>{t('modal.modelsUrl')}</span>
                  <code>{modelEndpoint}</code>
                </div>
              )}
              {modelOptionList.length > 0 && (
                <div className="model-option-panel">
                  <div className="model-option-toolbar">
                    <div>
                      <strong>{t('modal.modelSummary', { selected: provider.models.length, total: modelOptionList.length })}</strong>
                      <span>{t('modal.modelVisibleSummary', { visible: filteredModelOptions.length, selected: visibleSelectedCount })}</span>
                    </div>
                    <label className="model-search-field">
                      <Icon name="search" size={14} />
                      <input
                        value={modelSearch}
                        onChange={(event) => setModelSearch(event.target.value)}
                        placeholder={t("modal.searchModels")}
                      />
                    </label>
                    <div className="model-option-actions">
                      <button className="text-button" type="button" onClick={() => setVisibleModelsChecked(true)}>
                        {t('modal.selectAll')}
                      </button>
                      <button className="text-button" type="button" onClick={() => setVisibleModelsChecked(false)}>
                        {t('modal.deselectAll')}
                      </button>
                      <button className="text-button danger-text" type="button" onClick={clearSelectedModels}>
                        {t('modal.clearSelected')}
                      </button>
                    </div>
                  </div>
                  <label className="model-selected-filter">
                    <input
                      type="checkbox"
                      checked={showSelectedModelsOnly}
                      onChange={(event) => setShowSelectedModelsOnly(event.target.checked)}
                    />
                    <span>{t('modal.showSelectedOnly')}</span>
                  </label>
                  <div className="model-option-list selectable">
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
                      <span className="model-sync-status muted-copy">{t('modal.noMatch')}</span>
                    )}
                  </div>
                </div>
              )}
              {modelStatus === 'waiting' && (
                <span className="model-sync-status muted-copy">{t('modal.waitingBaseUrl')}</span>
              )}
              {modelStatus === 'loading' && (
                <span className="model-sync-status muted-copy">{t('modal.syncingModels')}</span>
              )}
              {modelStatus === 'ready' && !modelOptionList.length && (
                <span className="model-sync-status muted-copy">{t('modal.noUpstreamModels')}</span>
              )}
              {modelStatus === 'error' && (
                <span className="model-sync-status muted-copy">{t('modal.syncFailed')}</span>
              )}
              <div className="chip-list editor-chips">{provider.models.map((model) => <span className="model-chip" key={model}>{model}<button type="button" aria-label={t('modal.removeModel', { model })} onClick={() => removeModel(model)}>×</button></span>)}</div>
            </div>
          </section>
          <section className="config-section provider-test-section">
            <div className="config-section-heading">
              <span className="eyebrow">{t('modal.testEyebrow')}</span>
              <h3>{t('modal.testTitle')}</h3>
            </div>
            <div className="provider-test-panel">
              <div className="provider-test-toolbar">
                <div className="field compact-field provider-test-model-field">
                  <span>{t('modal.model')}</span>
                  <SelectControl
                    mono
                    value={testModel}
                    options={
                      testModelOptions.length
                        ? testModelOptions.map((model) => ({ value: model, label: model }))
                        : [{ value: '', label: t('modal.addOrSyncFirst') }]
                    }
                    onChange={setTestModel}
                    disabled={!testModelOptions.length}
                    ariaLabel={t("modal.selectTestModel")}
                  />
                </div>
                <label className="stream-toggle provider-test-stream-toggle">
                  <input
                    type="checkbox"
                    checked={testStream}
                    onChange={(event) => setTestStream(event.target.checked)}
                  />
                  <span>{t('modal.streamOutput')}</span>
                </label>
              </div>

              <label className="field compact-field prompt-field">
                <span>{t('modal.prompt')}</span>
                <textarea
                  value={testPrompt}
                  onChange={(event) => setTestPrompt(event.target.value)}
                  placeholder={t("modal.promptPlaceholder")}
                />
              </label>

              <div className="provider-test-actions">
                <div className="provider-test-action-buttons">
                  <button
                    className="button button-secondary"
                    type="button"
                    disabled={testStatus === 'loading'}
                    onClick={testCurrentModel}
                  >
                    <Icon name="play" size={15} />
                    {testStatus === 'loading' ? t('modal.detecting') : t('modal.detectModel')}
                  </button>
                  <button className="button button-secondary" type="button" onClick={copyTestCurl}>
                    <Icon name="copy" size={15} />
                    {t('modal.copyCurl')}
                  </button>
                </div>
                {(testMessage || curlCopyStatus) && (
                  <div className={`test-status ${testStatus}`}>{curlCopyStatus || testMessage}</div>
                )}
              </div>

              <div className="provider-test-enable-actions">
                <div className="provider-test-enable-copy">
                  <strong className={provider.enabled ? 'is-enabled' : 'is-disabled'}>
                    {provider.enabled ? t('modal.enabled') : t('modal.disabled')}
                  </strong>
                  <span>
                    {testStatus === 'ok'
                      ? t('modal.detectPassedEnable')
                      : testStatus === 'error'
                        ? t('modal.detectFailedDisable')
                        : t('modal.toggleAnytime')}
                  </span>
                </div>
                <div className="provider-test-enable-buttons">
                  <button
                    className={`button button-secondary ${provider.enabled ? 'is-active-enable' : ''}`}
                    type="button"
                    disabled={provider.enabled}
                    onClick={() => onChange({ ...provider, enabled: true })}
                  >
                    <Icon name="check" size={15} />
                    {t('modal.enable')}
                  </button>
                  <button
                    className={`button button-secondary ${!provider.enabled ? 'is-active-disable' : ''}`}
                    type="button"
                    disabled={!provider.enabled}
                    onClick={() => onChange({ ...provider, enabled: false })}
                  >
                    {t('modal.disable')}
                  </button>
                </div>
              </div>

              <div className="raw-response-panel">
                <div className="raw-response-heading">
                  <span>{t('modal.rawResponse')}</span>
                  <small>
                    {testRawData
                      ? `${testRawData.length} chars`
                      : testStream
                        ? 'stream'
                        : 'non-stream'}
                  </small>
                </div>
                <pre>{testRawData || t('modal.noResponseData')}</pre>
              </div>
            </div>
          </section>
        </div>
        <div className="modal-actions"><button className="button button-secondary" type="button" onClick={onClose}>{t('modal.cancel')}</button><button className="button button-primary" type="submit" disabled={!canSaveProvider}><Icon name="check" size={16} />{t('modal.saveConfig')}</button></div>
      </form>
    </div>
  )
}
