import { useCallback, useEffect, useMemo, useState } from 'react'
import type { FormEvent } from 'react'
import {
  defaultPriority,
  normalizePriority,
  normalizeProviderKind,
} from '../config/providers'
import { apiAuthHeaders, fetchModelCatalog } from '../lib/api'
import { downloadJson } from '../lib/browser'
import {
  buildConfigExport,
  dedupeProvidersForSave,
  filterModelAliases,
  fromApiProvider,
  modelAliasesFromImport,
  providersFromImport,
  toApiProvider,
} from '../lib/configImportExport'
import { moveItemByAction, reorderItem, reorderSameKindProviders } from '../lib/list'
import type { ListMoveAction } from '../lib/list'
import {
  persistAccessKey,
  readStoredAccessKey,
} from '../lib/storage'
import type {
  ApiModel,
  AuthStatus,
  DashboardPayload,
  Provider,
  ProviderDraft,
  ProviderKind,
  RetryConfig,
} from '../types/domain'

type PersistInput = {
  providers?: Provider[]
  priority?: ProviderKind[]
  fallbacks?: string[]
  modelAliases?: Record<string, string>
  retry?: RetryConfig
  apiKey?: string
}

export function useDashboardConfig(setToast: (message: string) => void) {
  const [providers, setProviders] = useState<Provider[]>([])
  const [priority, setPriority] = useState<ProviderKind[]>(defaultPriority)
  const [fallbacks, setFallbacks] = useState<string[]>([])
  const [modelAliases, setModelAliases] = useState<Record<string, string>>({})
  const [retry, setRetry] = useState<RetryConfig>({
    maxRetries: 5,
    backoffStepMs: 5000,
  })
  const [remoteModels, setRemoteModels] = useState<string[]>([])
  const [modelCatalog, setModelCatalog] = useState<ApiModel[]>([])
  const [accessKey, setAccessKey] = useState(() => readStoredAccessKey())
  const [authInput, setAuthInput] = useState(() => readStoredAccessKey())
  const [authStatus, setAuthStatus] = useState<AuthStatus>('checking')
  const [projectApiKey, setProjectApiKey] = useState('')
  const [port, setPort] = useState(3000)
  const [isSaving, setIsSaving] = useState(false)
  const [showApiKeyEditor, setShowApiKeyEditor] = useState(false)

  const configExport = useMemo(
    () => buildConfigExport(providers, priority, fallbacks, modelAliases, retry, projectApiKey, port),
    [fallbacks, modelAliases, port, priority, projectApiKey, providers, retry],
  )

  const applyPayload = useCallback((payload: DashboardPayload) => {
    setProviders(payload.providers.map(fromApiProvider))
    setPriority(normalizePriority(payload.model_priority))
    setFallbacks(payload.fallback_models)
    setModelAliases(
      filterModelAliases(
        payload.model_aliases,
        payload.providers.flatMap((item) => item.models),
      ),
    )
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
  }, [accessKey, applyPayload, setToast])

  useEffect(() => {
    void loadConfig()
  }, [loadConfig])

  const persistConfig = useCallback(
    async (next: PersistInput) => {
      const sourceProviders = next.providers ?? providers
      const nextProviders = dedupeProvidersForSave(sourceProviders)
      const nextPriority = next.priority ?? priority
      const nextFallbacks = next.fallbacks ?? fallbacks
      const configuredModels = new Set(nextProviders.flatMap((provider) => provider.models))
      const nextModelAliases = filterModelAliases(next.modelAliases ?? modelAliases, Array.from(configuredModels))
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
            model_aliases: nextModelAliases,
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
    [accessKey, applyPayload, fallbacks, modelAliases, priority, projectApiKey, providers, retry, setToast],
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

  const configuredModels = useMemo(
    () => Array.from(new Set(providers.flatMap((provider) => provider.models))).sort(),
    [providers],
  )

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

  function saveProvider(editingId: string | null, editingProvider: ProviderDraft) {
    if (!editingProvider.name.trim()) return false
    const nextProviders = editingId
      ? providers.map((provider) =>
          provider.id === editingId ? { ...editingProvider, id: editingId } : provider,
        )
      : [...providers, { ...editingProvider, id: `${editingProvider.kind}:${Date.now()}` }]
    setProviders(nextProviders)
    void persistConfig({ providers: nextProviders })
    return true
  }

  function deleteProvider(id: string) {
    const nextProviders = providers.filter((item) => item.id !== id)
    setProviders(nextProviders)
    void persistConfig({ providers: nextProviders })
  }

  function moveProviderConfig(id: string, action: ListMoveAction) {
    const provider = providers.find((item) => item.id === id)
    if (!provider) return
    const kindProviders = providers.filter((item) => item.kind === provider.kind)
    const index = kindProviders.findIndex((item) => item.id === id)
    if (index < 0) return
    const targetIndex =
      action === 'top' ? 0 : action === 'bottom' ? kindProviders.length - 1 : index + action
    const target = kindProviders[targetIndex]
    if (!target || target.id === id) return
    reorderProviderConfig(id, target.id)
  }

  function reorderProviderConfig(sourceId: string, targetId: string) {
    const nextProviders = reorderSameKindProviders(providers, sourceId, targetId)
    if (!nextProviders) return
    setProviders(nextProviders as Provider[])
    void persistConfig({ providers: nextProviders as Provider[] })
  }

  function movePriority(index: number, action: ListMoveAction) {
    const nextPriority = moveItemByAction(priority, index, action)
    if (!nextPriority) return
    setPriority(nextPriority)
    void persistConfig({ priority: nextPriority })
  }

  function reorderPriority(sourceIndex: number, targetIndex: number) {
    const nextPriority = reorderItem(priority, sourceIndex, targetIndex)
    if (!nextPriority) return
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
    if (!nextModels.length) return false
    const nextFallbacks = [...fallbacks, ...nextModels]
    setFallbacks(nextFallbacks)
    void persistConfig({ fallbacks: nextFallbacks })
    return true
  }

  function updateModelAliases(nextAliases: Record<string, string>) {
    const normalized = filterModelAliases(nextAliases, configuredModels)
    setModelAliases(normalized)
    void persistConfig({ modelAliases: normalized })
  }

  function removeFallback(model: string) {
    const nextFallbacks = fallbacks.filter((item) => item !== model)
    setFallbacks(nextFallbacks)
    void persistConfig({ fallbacks: nextFallbacks })
  }

  function moveFallback(index: number, action: ListMoveAction) {
    const targetIndex =
      action === 'top' ? 0 : action === 'bottom' ? fallbacks.length - 1 : index + action
    reorderFallback(index, targetIndex)
  }

  function reorderFallback(sourceIndex: number, targetIndex: number) {
    const nextFallbacks = reorderItem(fallbacks, sourceIndex, targetIndex)
    if (!nextFallbacks) return
    setFallbacks(nextFallbacks)
    void persistConfig({ fallbacks: nextFallbacks })
  }

  function exportConfig() {
    downloadJson('llm-proxy-config.json', configExport)
  }

  async function importConfigFile(file: File) {
    try {
      const raw = await file.text()
      const value = JSON.parse(raw)
      const imported = providersFromImport(value)
      const importedAliases = filterModelAliases(
        modelAliasesFromImport(value),
        [...providers, ...imported].flatMap((provider) => provider.models),
      )
      if (!imported.length && !Object.keys(importedAliases).length) {
        setToast('没有发现可导入的配置')
        return
      }
      const nextProviders = [...providers, ...imported]
      const nextAliases = { ...modelAliases, ...importedAliases }
      setProviders(nextProviders)
      setModelAliases(nextAliases)
      void persistConfig({ providers: nextProviders, modelAliases: nextAliases })
      setToast(`已导入 ${imported.length} 个提供商，${Object.keys(importedAliases).length} 个别名`)
    } catch {
      setToast('导入失败，请检查 JSON 格式')
    }
  }

  return {
    providers,
    setProviders,
    priority,
    fallbacks,
    modelAliases,
    retry,
    accessKey,
    authInput,
    setAuthInput,
    authStatus,
    setAuthStatus,
    projectApiKey,
    port,
    isSaving,
    showApiKeyEditor,
    setShowApiKeyEditor,
    allModels,
    configuredModels,
    modelsByProviderKind,
    persistConfig,
    loginWithApiKey,
    toggleProvider,
    saveProvider,
    deleteProvider,
    moveProviderConfig,
    reorderProviderConfig,
    movePriority,
    reorderPriority,
    addFallbackModel,
    addFallbackModels,
    updateModelAliases,
    removeFallback,
    moveFallback,
    reorderFallback,
    exportConfig,
    importConfigFile,
  }
}
