import { useCallback, useEffect, useMemo, useState } from 'react'
import type { FormEvent } from 'react'
import './App.css'
import { Icon } from './components/Icon'
import { ThemeSwitcher } from './components/ThemeSwitcher'
import { AccessCheckingOverlay, LoginOverlay } from './components/auth/AccessOverlays'
import { NavItem, ProviderNavGroups } from './components/navigation'
import {
  defaultBaseUrlForNewProvider,
  defaultPriority,
  normalizePriority,
  normalizeProviderKind,
  providerMeta,
} from './config/providers'
import { ApiKeyModal } from './features/modals/ApiKeyModal'
import { FallbackModelModal } from './features/modals/FallbackModelModal'
import { ProviderModal } from './features/modals/ProviderModal'
import { ProvidersView } from './features/providers/ProvidersView'
import { RoutingView } from './features/routing/RoutingView'
import { apiAuthHeaders, fetchModelCatalog } from './lib/api'
import {
  applyAuthValidationResults,
  buildAuthValidationConfig,
  deleteProviderAuthTarget,
  mergeAuthValidationState,
  setProviderAuthDisabled,
  syncAuthValidationPayloadWithProviders,
  visibleAuthValidationResults,
} from './lib/authValidation'
import { downloadJson } from './lib/browser'
import {
  buildConfigExport,
  dedupeProvidersForSave,
  fromApiProvider,
  providersFromImport,
  toApiProvider,
} from './lib/configImportExport'
import {
  persistAccessKey,
  readStoredAccessKey,
  readStoredThemeMode,
} from './lib/storage'
import type {
  ApiModel,
  AuthProviderKind,
  AuthStatus,
  AuthValidationFilter,
  AuthValidationResponse,
  AuthValidationState,
  AuthValidationTarget,
  DashboardPayload,
  Provider,
  ProviderDraft,
  ProviderKind,
  ProviderKindFilter,
  RetryConfig,
  View,
} from './types/domain'

function App() {
  const [view, setView] = useState<View>('providers')
  const [themeMode, setThemeMode] = useState(() => readStoredThemeMode())
  const [providers, setProviders] = useState<Provider[]>([])
  const [priority, setPriority] = useState<ProviderKind[]>(defaultPriority)
  const [fallbacks, setFallbacks] = useState<string[]>([])
  const [retry, setRetry] = useState<RetryConfig>({
    maxRetries: 5,
    backoffStepMs: 5000,
  })
  const [remoteModels, setRemoteModels] = useState<string[]>([])
  const [modelCatalog, setModelCatalog] = useState<ApiModel[]>([])
  const [query, setQuery] = useState('')
  const [providerFilter, setProviderFilter] = useState<'all' | 'enabled'>('all')
  const [providerKindFilter, setProviderKindFilter] = useState<ProviderKindFilter>('all')
  const [providerNavCollapsed, setProviderNavCollapsed] = useState(false)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [editingProvider, setEditingProvider] = useState<ProviderDraft | null>(null)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [showAddFallback, setShowAddFallback] = useState(false)
  const [showApiKeyEditor, setShowApiKeyEditor] = useState(false)
  const [accessKey, setAccessKey] = useState(() => readStoredAccessKey())
  const [authInput, setAuthInput] = useState(() => readStoredAccessKey())
  const [authStatus, setAuthStatus] = useState<AuthStatus>('checking')
  const [projectApiKey, setProjectApiKey] = useState('')
  const [port, setPort] = useState(3000)
  const [toast, setToast] = useState('')
  const [isSaving, setIsSaving] = useState(false)
  const [validatingAuthKind, setValidatingAuthKind] = useState<ProviderKind | null>(null)
  const [authValidation, setAuthValidation] = useState<AuthValidationState | null>(null)
  const configExport = useMemo(
    () => buildConfigExport(providers, priority, fallbacks, retry, projectApiKey, port),
    [fallbacks, port, priority, projectApiKey, providers, retry],
  )

  const applyPayload = useCallback((payload: DashboardPayload) => {
    setProviders(payload.providers.map(fromApiProvider))
    setPriority(normalizePriority(payload.model_priority))
    setFallbacks(payload.fallback_models)
    setProjectApiKey(payload.api_key ?? '')
    setPort(typeof payload.port === 'number' && payload.port > 0 ? payload.port : 3000)
    setRetry({
      maxRetries: payload.retry.max_retries,
      backoffStepMs: payload.retry.backoff_step_ms,
    })
  }, [])

  const loadConfig = useCallback(async () => {
    try {
      const response = await fetch('/api/config', {
        headers: apiAuthHeaders(accessKey),
        signal: AbortSignal.timeout(5000),
      })
      if (response.status === 401) {
        setAuthStatus('login')
        return
      }
      if (!response.ok) throw new Error(await response.text())
      const payload = (await response.json()) as DashboardPayload
      applyPayload(payload)
      setAuthStatus('ready')
      const catalog = await fetchModelCatalog(accessKey)
      setModelCatalog(catalog)
      setRemoteModels(
        Array.from(
          new Set([
            ...payload.providers.flatMap((item) => item.models),
            ...catalog.map((item) => item.id),
          ]),
        ),
      )
    } catch {
      setAuthStatus('login')
      setToast('配置加载失败，请检查后端日志')
    }
  }, [accessKey, applyPayload])

  useEffect(() => {
    void loadConfig()
  }, [loadConfig])

  useEffect(() => {
    const media = window.matchMedia('(prefers-color-scheme: dark)')
    const applyTheme = () => {
      const resolvedTheme = themeMode === 'system'
        ? media.matches ? 'dark' : 'light'
        : themeMode
      document.documentElement.dataset.theme = resolvedTheme
      document.documentElement.dataset.themeMode = themeMode
      window.localStorage.setItem('llm-proxy-theme', themeMode)
    }
    applyTheme()
    media.addEventListener('change', applyTheme)
    return () => media.removeEventListener('change', applyTheme)
  }, [themeMode])

  useEffect(() => {
    if (!toast) return
    const timer = window.setTimeout(() => setToast(''), 2200)
    return () => window.clearTimeout(timer)
  }, [toast])

  const persistConfig = useCallback(
    async (next: {
      providers?: Provider[]
      priority?: ProviderKind[]
      fallbacks?: string[]
      retry?: RetryConfig
      apiKey?: string
    }) => {
      const sourceProviders = next.providers ?? providers
      const nextProviders = dedupeProvidersForSave(sourceProviders)
      const nextPriority = next.priority ?? priority
      const nextFallbacks = next.fallbacks ?? fallbacks
      const nextRetry = next.retry ?? retry
      const nextApiKey = next.apiKey ?? projectApiKey
      if (nextProviders.length !== sourceProviders.length) {
        setProviders(nextProviders)
      }
      setIsSaving(true)
      try {
        const response = await fetch('/api/config', {
          method: 'PUT',
          headers: apiAuthHeaders(accessKey, { 'content-type': 'application/json' }),
          body: JSON.stringify({
            providers: nextProviders.map(toApiProvider),
            model_priority: nextPriority,
            fallback_models: nextFallbacks,
            api_key: nextApiKey,
            retry: {
              max_retries: nextRetry.maxRetries,
              backoff_step_ms: nextRetry.backoffStepMs,
            },
          }),
        })
        if (response.status === 401) {
          setAuthStatus('login')
          throw new Error('unauthorized')
        }
        if (!response.ok) throw new Error(await response.text())
        const payload = (await response.json()) as DashboardPayload
        applyPayload(payload)
        if (next.apiKey !== undefined) {
          persistAccessKey(nextApiKey)
          setAccessKey(nextApiKey)
          setAuthInput(nextApiKey)
          setShowApiKeyEditor(false)
          setAuthStatus('ready')
        }
        setToast('配置已保存')
      } catch {
        setToast('保存失败，请检查后端日志')
      } finally {
        setIsSaving(false)
      }
    },
    [accessKey, applyPayload, fallbacks, priority, projectApiKey, providers, retry],
  )

  async function loginWithApiKey(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    const nextKey = authInput.trim()
    try {
      const response = await fetch('/api/config', {
        headers: apiAuthHeaders(nextKey),
        signal: AbortSignal.timeout(5000),
      })
      if (response.status === 401) {
        setToast('API Key 不正确')
        return
      }
      if (!response.ok) throw new Error(await response.text())
      const payload = (await response.json()) as DashboardPayload
      persistAccessKey(nextKey)
      setAccessKey(nextKey)
      setAuthStatus('ready')
      applyPayload(payload)
      const catalog = await fetchModelCatalog(nextKey)
      setModelCatalog(catalog)
      setRemoteModels(
        Array.from(
          new Set([
            ...payload.providers.flatMap((item) => item.models),
            ...catalog.map((item) => item.id),
          ]),
        ),
      )
    } catch {
      setToast('登录失败，请检查后端日志')
    }
  }

  const allModels = useMemo(() => {
    const values = new Set(providers.flatMap((provider) => provider.models))
    remoteModels.forEach((model) => values.add(model))
    return Array.from(values).sort()
  }, [providers, remoteModels])

  const modelsByProviderKind = useMemo(() => {
    const groups = Object.fromEntries(
      defaultPriority.map((kind) => [kind, new Set<string>()]),
    ) as Record<ProviderKind, Set<string>>
    for (const provider of providers) {
      for (const model of provider.models) {
        groups[provider.kind].add(model)
      }
    }
    for (const item of modelCatalog) {
      const kind = normalizeProviderKind(item.owned_by)
      if (kind) groups[kind].add(item.id)
    }
    return Object.fromEntries(
      defaultPriority.map((kind) => [kind, Array.from(groups[kind]).sort()]),
    ) as Record<ProviderKind, string[]>
  }, [modelCatalog, providers])

  function toggleProvider(id: string) {
    const nextProviders = providers.map((provider) =>
      provider.id === id ? { ...provider, enabled: !provider.enabled } : provider,
    )
    setProviders(nextProviders)
    void persistConfig({ providers: nextProviders })
  }

  function openEditor(provider?: Provider, preferredKind?: ProviderKind) {
    if (provider) {
      setEditingId(provider.id)
      setEditingProvider({ ...provider })
      return
    }
    const kind = preferredKind && defaultPriority.includes(preferredKind)
      ? preferredKind
      : 'openai_chat'
    setEditingId(null)
    setEditingProvider({
      kind,
      name: 'New provider',
      baseUrl: defaultBaseUrlForNewProvider(kind),
      apiKey: '',
      models: [],
      enabled: true,
    })
  }

  function copyProvider(provider: Provider) {
    setEditingId(null)
    setEditingProvider({
      ...provider,
      name: `${provider.name} copy`,
      kind: provider.kind,
    })
  }

  function saveProvider(event: FormEvent<HTMLFormElement>) {
    event.preventDefault()
    if (!editingProvider || !editingProvider.name.trim()) return
    const nextProviders = editingId
      ? providers.map((provider) =>
          provider.id === editingId ? { ...editingProvider, id: editingId } : provider,
        )
      : [...providers, { ...editingProvider, id: `${editingProvider.kind}:${Date.now()}` }]
    setProviders(nextProviders)
    setEditingProvider(null)
    void persistConfig({ providers: nextProviders })
  }

  function deleteProvider(id: string) {
    const nextProviders = providers.filter((item) => item.id !== id)
    setProviders(nextProviders)
    void persistConfig({ providers: nextProviders })
  }

  function moveProviderConfig(id: string, direction: -1 | 1) {
    const provider = providers.find((item) => item.id === id)
    if (!provider) return
    const kindProviders = providers.filter((item) => item.kind === provider.kind)
    const index = kindProviders.findIndex((item) => item.id === id)
    if (index < 0) return
    const target = kindProviders[index + direction]
    if (!target) return
    reorderProviderConfig(id, target.id)
  }

  function reorderProviderConfig(sourceId: string, targetId: string) {
    if (!sourceId || !targetId || sourceId === targetId) return
    const source = providers.find((item) => item.id === sourceId)
    const target = providers.find((item) => item.id === targetId)
    if (!source || !target || source.kind !== target.kind) return

    const kindProviders = providers.filter((item) => item.kind === source.kind)
    const sourceIndex = kindProviders.findIndex((item) => item.id === sourceId)
    const targetIndex = kindProviders.findIndex((item) => item.id === targetId)
    if (sourceIndex < 0 || targetIndex < 0 || sourceIndex === targetIndex) return

    const reordered = [...kindProviders]
    const [moved] = reordered.splice(sourceIndex, 1)
    reordered.splice(targetIndex, 0, moved)

    let cursor = 0
    const nextProviders = providers.map((item) =>
      item.kind === source.kind ? reordered[cursor++] : item,
    )
    setProviders(nextProviders)
    void persistConfig({ providers: nextProviders })
  }

  async function validateAuths(
    kind: AuthProviderKind,
    options: { targets?: AuthValidationTarget[]; replace?: boolean } = {},
  ) {
    const kindProviders = providers.filter((provider) => provider.kind === kind)
    if (!kindProviders.length) {
      setToast(`没有可校验的 ${providerMeta[kind].label} 提供商`)
      return
    }
    if (options.targets && !options.targets.length) {
      setToast('当前筛选项没有可校验的 auth')
      return
    }
    setValidatingAuthKind(kind)
    if (options.replace !== false) setAuthValidation(null)
    try {
      const response = await fetch(`/api/${kind}/validate`, {
        method: 'POST',
        headers: apiAuthHeaders(accessKey, { 'content-type': 'application/json' }),
        body: JSON.stringify({
          config: buildAuthValidationConfig(providers),
          ...(options.targets ? { targets: options.targets } : {}),
        }),
      })
      if (response.status === 401) {
        setAuthStatus('login')
        throw new Error('unauthorized')
      }
      if (!response.ok) throw new Error(await response.text())
      const payload = (await response.json()) as AuthValidationResponse
      if (!payload.success) throw new Error('validation failed')
      const nextProviders = applyAuthValidationResults(providers, kind, payload.data.results)
      const nextValidation = mergeAuthValidationState(
        options.replace === false ? authValidation : null,
        kind,
        payload.data,
      )
      setProviders(nextProviders)
      setAuthValidation(nextValidation)
      void persistConfig({ providers: nextProviders })
      setToast(
        `${providerMeta[kind].label} 校验完成：有效 ${nextValidation.payload.valid}，无效 ${nextValidation.payload.invalid}`,
      )
    } catch (error) {
      if (!(error instanceof Error && error.message === 'unauthorized')) {
        setToast(`${providerMeta[kind].label} auth 校验失败`)
      }
    } finally {
      setValidatingAuthKind(null)
    }
  }

  function setAuthValidationFilter(filter: AuthValidationFilter) {
    setAuthValidation((current) => current ? { ...current, filter } : current)
  }

  function validateAuthTargets(kind: AuthProviderKind, targets?: AuthValidationTarget[]) {
    void validateAuths(kind, targets ? { targets, replace: false } : {})
  }

  function updateAuthValidationProviders(
    kind: AuthProviderKind,
    updater: (providers: Provider[]) => Provider[],
    message: string,
  ) {
    const nextProviders = updater(providers)
    setProviders(nextProviders)
    setAuthValidation((current) =>
      current?.kind === kind
        ? { ...current, payload: syncAuthValidationPayloadWithProviders(current.payload, nextProviders, kind) }
        : current,
    )
    void persistConfig({ providers: nextProviders })
    setToast(message)
  }

  function disableAuthFromValidation(kind: AuthProviderKind, target: AuthValidationTarget, disabled: boolean) {
    updateAuthValidationProviders(
      kind,
      (currentProviders) => setProviderAuthDisabled(currentProviders, kind, target, disabled),
      disabled ? '已禁用 auth，配置保存中' : '已启用 auth，配置保存中',
    )
  }

  function deleteAuthFromValidation(kind: AuthProviderKind, target: AuthValidationTarget) {
    updateAuthValidationProviders(
      kind,
      (currentProviders) => deleteProviderAuthTarget(currentProviders, kind, target),
      '已删除 auth，配置保存中',
    )
  }

  function validateVisibleAuthResults() {
    if (!authValidation) return
    const targets = visibleAuthValidationResults(authValidation).map((result) => ({
      providerIndex: result.providerIndex,
      authIndex: result.authIndex,
    }))
    void validateAuths(authValidation.kind, { targets, replace: false })
  }

  function disableVisibleAuthResults() {
    if (!authValidation) return
    const targets = visibleAuthValidationResults(authValidation)
    if (!targets.length) {
      setToast('当前筛选项没有可禁用的 auth')
      return
    }
    updateAuthValidationProviders(
      authValidation.kind,
      (currentProviders) => targets.reduce(
        (nextProviders, result) => setProviderAuthDisabled(nextProviders, authValidation.kind, result, true),
        currentProviders,
      ),
      `已禁用 ${targets.length} 个 auth，配置保存中`,
    )
  }

  function enableVisibleAuthResults() {
    if (!authValidation) return
    const targets = visibleAuthValidationResults(authValidation)
    if (!targets.length) {
      setToast('当前筛选项没有可启用的 auth')
      return
    }
    updateAuthValidationProviders(
      authValidation.kind,
      (currentProviders) => targets.reduce(
        (nextProviders, result) => setProviderAuthDisabled(nextProviders, authValidation.kind, result, false),
        currentProviders,
      ),
      `已启用 ${targets.length} 个 auth，配置保存中`,
    )
  }

  function deleteVisibleAuthResults() {
    if (!authValidation) return
    const targets = visibleAuthValidationResults(authValidation)
    if (!targets.length) {
      setToast('当前筛选项没有可删除的 auth')
      return
    }
    const orderedTargets = [...targets].sort(
      (a, b) => b.providerIndex - a.providerIndex || b.authIndex - a.authIndex,
    )
    updateAuthValidationProviders(
      authValidation.kind,
      (currentProviders) => orderedTargets.reduce(
        (nextProviders, result) => deleteProviderAuthTarget(nextProviders, authValidation.kind, result),
        currentProviders,
      ),
      `已删除 ${targets.length} 个 auth，配置保存中`,
    )
  }

  function exportConfig() {
    downloadJson('llm-proxy-config.json', configExport)
  }

  async function importConfigFile(file: File) {
    try {
      const raw = await file.text()
      const value = JSON.parse(raw)
      const imported = providersFromImport(value)
      if (!imported.length) {
        setToast('没有发现可导入的提供商')
        return
      }
      const nextProviders = [...providers, ...imported]
      setProviders(nextProviders)
      void persistConfig({ providers: nextProviders })
      setToast(`已导入 ${imported.length} 个提供商`)
    } catch {
      setToast('导入失败，请检查 JSON 格式')
    }
  }

  function movePriority(index: number, direction: -1 | 1) {
    const targetIndex = index + direction
    if (targetIndex < 0 || targetIndex >= priority.length) return
    reorderPriority(index, targetIndex)
  }

  function reorderPriority(sourceIndex: number, targetIndex: number) {
    if (
      sourceIndex === targetIndex ||
      sourceIndex < 0 ||
      targetIndex < 0 ||
      sourceIndex >= priority.length ||
      targetIndex >= priority.length
    ) {
      return
    }
    const nextPriority = [...priority]
    const [moved] = nextPriority.splice(sourceIndex, 1)
    nextPriority.splice(targetIndex, 0, moved)
    setPriority(nextPriority)
    void persistConfig({ priority: nextPriority })
  }

  function addFallbackModel(model: string) {
    if (!model || fallbacks.includes(model)) return
    const nextFallbacks = [...fallbacks, model]
    setFallbacks(nextFallbacks)
    void persistConfig({ fallbacks: nextFallbacks })
  }

  function addFallbackModels(models: string[]) {
    const seen = new Set(fallbacks)
    const nextModels = models
      .map((model) => model.trim())
      .filter((model) => {
        if (!model || seen.has(model)) return false
        seen.add(model)
        return true
      })
    if (!nextModels.length) return
    const nextFallbacks = [...fallbacks, ...nextModels]
    setFallbacks(nextFallbacks)
    setShowAddFallback(false)
    void persistConfig({ fallbacks: nextFallbacks })
  }

  function openFallbackEditor() {
    setShowAddFallback(true)
  }

  function removeFallback(model: string) {
    const nextFallbacks = fallbacks.filter((item) => item !== model)
    setFallbacks(nextFallbacks)
    void persistConfig({ fallbacks: nextFallbacks })
  }

  function moveFallback(index: number, direction: -1 | 1) {
    const targetIndex = index + direction
    if (targetIndex < 0 || targetIndex >= fallbacks.length) return
    reorderFallback(index, targetIndex)
  }

  function reorderFallback(sourceIndex: number, targetIndex: number) {
    if (
      sourceIndex === targetIndex ||
      sourceIndex < 0 ||
      targetIndex < 0 ||
      sourceIndex >= fallbacks.length ||
      targetIndex >= fallbacks.length
    ) {
      return
    }
    const nextFallbacks = [...fallbacks]
    const [moved] = nextFallbacks.splice(sourceIndex, 1)
    nextFallbacks.splice(targetIndex, 0, moved)
    setFallbacks(nextFallbacks)
    void persistConfig({ fallbacks: nextFallbacks })
  }

  if (authStatus === 'checking') {
    return (
      <>
        <AccessCheckingOverlay />
        {toast && <div className="toast">{toast}</div>}
      </>
    )
  }

  if (authStatus === 'login') {
    return (
      <>
        <LoginOverlay
          value={authInput}
          onChange={setAuthInput}
          onSubmit={loginWithApiKey}
        />
        {toast && <div className="toast">{toast}</div>}
      </>
    )
  }

  const pageTitle: Record<View, string> = {
    providers: '提供商',
    routing: '模型路由',
  }

  return (
    <div className={`app-shell ${sidebarCollapsed ? 'sidebar-is-collapsed' : ''}`}>
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">L</div>
          <div>
            <strong>llm-proxy</strong>
            <span>control center</span>
          </div>
          <button
            className="sidebar-collapse-button"
            type="button"
            title={sidebarCollapsed ? '展开侧边栏' : '折叠侧边栏'}
            aria-label={sidebarCollapsed ? '展开侧边栏' : '折叠侧边栏'}
            onClick={() => setSidebarCollapsed((collapsed) => !collapsed)}
          >
            <Icon name="sidebar" size={17} />
          </button>
        </div>

        <nav className="nav-list" aria-label="主导航">
          <NavItem
            icon="server"
            label="提供商"
            count={providers.length}
            active={view === 'providers'}
            expanded={!providerNavCollapsed}
            onClick={() => {
              setProviderNavCollapsed((collapsed) =>
                view === 'providers' ? !collapsed : false,
              )
              setProviderKindFilter('all')
              setView('providers')
            }}
          />
          <ProviderNavGroups
            providers={providers}
            activeKind={view === 'providers' ? providerKindFilter : 'all'}
            collapsed={providerNavCollapsed || sidebarCollapsed}
            onSelect={(kind) => {
              setProviderKindFilter(kind)
              setView('providers')
            }}
          />
          <NavItem
            icon="route"
            label="模型路由"
            active={view === 'routing'}
            onClick={() => setView('routing')}
          />
        </nav>

        <div className="sidebar-bottom">
          <div className="sidebar-footer">
            <span>v0.1.0</span>
            <span className="footer-dot">•</span>
            <span>Rust runtime</span>
          </div>
        </div>
      </aside>

      <main className="main-content">
        <header className="topbar">
          <div>
            <div className="breadcrumb">
              <span>Embedded console</span>
              <Icon name="chevron" size={13} />
              <strong>{pageTitle[view]}</strong>
            </div>
            <h1>{pageTitle[view]}</h1>
          </div>
          <div className="topbar-actions">
            <ThemeSwitcher value={themeMode} onChange={setThemeMode} />
            <button className="button button-secondary" type="button" onClick={() => setShowApiKeyEditor(true)}>
              <Icon name="key" size={16} />
              API Key
            </button>
            <label className="button button-secondary import-button">
              <Icon name="upload" size={16} />
              导入配置
              <input
                type="file"
                accept="application/json,.json"
                onChange={(event) => {
                  const file = event.target.files?.[0]
                  if (file) void importConfigFile(file)
                  event.currentTarget.value = ''
                }}
              />
            </label>
            <button className="button button-secondary" type="button" onClick={exportConfig}>
              <Icon name="download" size={16} />
              导出配置
            </button>
            {isSaving && <span className="saving-label">正在保存</span>}
          </div>
        </header>

        <div className="page-content">
          {view === 'providers' && (
            <ProvidersView
              providers={providers}
              query={query}
              filter={providerFilter}
              kindFilter={providerKindFilter}
              onQueryChange={setQuery}
              onFilterChange={setProviderFilter}
              onKindFilterChange={setProviderKindFilter}
              onToggle={toggleProvider}
              onEdit={openEditor}
              onCopy={copyProvider}
              onDelete={deleteProvider}
              onMoveProvider={moveProviderConfig}
              onReorderProvider={reorderProviderConfig}
              onAdd={() => openEditor(undefined, providerKindFilter === 'all' ? undefined : providerKindFilter)}
              onValidateAuths={validateAuthTargets}
              validatingAuthKind={validatingAuthKind}
              authValidation={authValidation}
              onAuthValidationFilterChange={setAuthValidationFilter}
              onValidateVisibleAuths={validateVisibleAuthResults}
              onEnableVisibleAuths={enableVisibleAuthResults}
              onDisableVisibleAuths={disableVisibleAuthResults}
              onDeleteVisibleAuths={deleteVisibleAuthResults}
              onValidateAuthResult={(kind, target) => void validateAuths(kind, { targets: [target], replace: false })}
              onDisableAuthResult={disableAuthFromValidation}
              onDeleteAuthResult={deleteAuthFromValidation}
            />
          )}
          {view === 'routing' && (
            <RoutingView
              priority={priority}
              fallbacks={fallbacks}
              allModels={allModels}
              modelsByProviderKind={modelsByProviderKind}
              providers={providers}
              onMove={movePriority}
              onReorder={reorderPriority}
              onRemoveFallback={removeFallback}
              onMoveFallback={moveFallback}
              onReorderFallback={reorderFallback}
              onAddFallbackModel={addFallbackModel}
              onAddFallback={openFallbackEditor}
            />
          )}
        </div>
      </main>

      {editingProvider && (
        <ProviderModal
          provider={editingProvider}
          isEditing={Boolean(editingId)}
          allowKindChange={!editingId}
          accessKey={accessKey}
          onChange={setEditingProvider}
          onClose={() => setEditingProvider(null)}
          onSubmit={saveProvider}
        />
      )}
      {showAddFallback && (
        <FallbackModelModal
          allModels={allModels}
          fallbacks={fallbacks}
          onClose={() => setShowAddFallback(false)}
          onSubmit={addFallbackModels}
        />
      )}
      {showApiKeyEditor && (
        <ApiKeyModal
          apiKey={projectApiKey}
          onClose={() => setShowApiKeyEditor(false)}
          onSubmit={(apiKey) => void persistConfig({ apiKey })}
        />
      )}
      {toast && <div className="toast">{toast}</div>}
    </div>
  )
}

export default App
