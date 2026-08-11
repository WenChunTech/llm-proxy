import { useState } from 'react'
import type { FormEvent } from 'react'
import './App.css'
import { AccessCheckingOverlay, LoginOverlay } from './components/auth/AccessOverlays'
import { AppShell } from './components/layout/AppShell'
import {
  defaultBaseUrlForNewProvider,
  defaultPriority,
} from './config/providers'
import { ApiKeyModal } from './features/modals/ApiKeyModal'
import { FallbackModelModal } from './features/modals/FallbackModelModal'
import { ProviderModal } from './features/modals/ProviderModal'
import { ProvidersView } from './features/providers/ProvidersView'
import { LogsView } from './features/logs/LogsView'
import { RoutingView } from './features/routing/RoutingView'
import { useAuthValidation } from './hooks/useAuthValidation'
import { useDashboardConfig } from './hooks/useDashboardConfig'
import { useThemeMode } from './hooks/useThemeMode'
import { useToast } from './hooks/useToast'
import type {
  Provider,
  ProviderDraft,
  ProviderKind,
  ProviderKindFilter,
  View,
} from './types/domain'

function App() {
  const [view, setView] = useState<View>('providers')
  const { themeMode, setThemeMode } = useThemeMode()
  const { toast, setToast } = useToast()
  const [query, setQuery] = useState('')
  const [providerFilter, setProviderFilter] = useState<'all' | 'enabled'>('all')
  const [providerKindFilter, setProviderKindFilter] = useState<ProviderKindFilter>('all')
  const [providerNavCollapsed, setProviderNavCollapsed] = useState(false)
  const [sidebarCollapsed, setSidebarCollapsed] = useState(false)
  const [editingProvider, setEditingProvider] = useState<ProviderDraft | null>(null)
  const [editingId, setEditingId] = useState<string | null>(null)
  const [showAddFallback, setShowAddFallback] = useState(false)

  const config = useDashboardConfig(setToast)
  const authValidation = useAuthValidation({
    providers: config.providers,
    setProviders: config.setProviders,
    accessKey: config.accessKey,
    setAuthStatus: config.setAuthStatus,
    persistConfig: config.persistConfig,
    setToast,
  })

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
      headers: {},
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
    if (!editingProvider) return
    if (config.saveProvider(editingId, editingProvider)) {
      setEditingProvider(null)
    }
  }

  if (config.authStatus === 'checking') {
    return (
      <>
        <AccessCheckingOverlay />
        {toast && <div className="toast">{toast}</div>}
      </>
    )
  }

  if (config.authStatus === 'login') {
    return (
      <>
        <LoginOverlay
          value={config.authInput}
          onChange={config.setAuthInput}
          onSubmit={config.loginWithApiKey}
        />
        {toast && <div className="toast">{toast}</div>}
      </>
    )
  }

  return (
    <>
      <AppShell
        view={view}
        setView={setView}
        providers={config.providers}
        providerKindFilter={providerKindFilter}
        setProviderKindFilter={setProviderKindFilter}
        providerNavCollapsed={providerNavCollapsed}
        setProviderNavCollapsed={setProviderNavCollapsed}
        sidebarCollapsed={sidebarCollapsed}
        setSidebarCollapsed={setSidebarCollapsed}
        themeMode={themeMode}
        setThemeMode={setThemeMode}
        isSaving={config.isSaving}
        onOpenApiKey={() => config.setShowApiKeyEditor(true)}
        onImportFile={(file) => void config.importConfigFile(file)}
        onExport={config.exportConfig}
      >
        {view === 'providers' && (
          <ProvidersView
            providers={config.providers}
            query={query}
            filter={providerFilter}
            kindFilter={providerKindFilter}
            onQueryChange={setQuery}
            onFilterChange={setProviderFilter}
            onKindFilterChange={setProviderKindFilter}
            onToggle={config.toggleProvider}
            onEdit={openEditor}
            onCopy={copyProvider}
            onDelete={config.deleteProvider}
            onMoveProvider={config.moveProviderConfig}
            onReorderProvider={config.reorderProviderConfig}
            onAdd={() => openEditor(undefined, providerKindFilter === 'all' ? undefined : providerKindFilter)}
            onValidateAuths={authValidation.validateAuthTargets}
            isKindValidating={authValidation.isKindValidating}
            isProviderValidating={authValidation.isProviderValidating}
            isTargetValidating={authValidation.isTargetValidating}
            providerValidationProgress={authValidation.providerValidationProgress}
            authValidation={authValidation.authValidation}
            validationConcurrency={authValidation.validationConcurrency}
            onValidationConcurrencyChange={authValidation.setValidationConcurrency}
            onAuthValidationFilterChange={authValidation.setAuthValidationFilter}
            onClearAuthValidation={authValidation.clearAuthValidation}
            onValidateVisibleAuths={authValidation.validateVisibleAuthResults}
            onEnableVisibleAuths={authValidation.enableVisibleAuthResults}
            onDisableVisibleAuths={authValidation.disableVisibleAuthResults}
            onDeleteVisibleAuths={authValidation.deleteVisibleAuthResults}
            onValidateAuthResult={(kind, target) =>
              void authValidation.validateAuths(kind, { targets: [target], replace: false })
            }
            onDisableAuthResult={authValidation.disableAuthFromValidation}
            onDeleteAuthResult={authValidation.deleteAuthFromValidation}
            setToast={setToast}
          />
        )}
        {view === 'routing' && (
          <RoutingView
            priority={config.priority}
            fallbacks={config.fallbacks}
            allModels={config.allModels}
            configuredModels={config.configuredModels}
            modelsByProviderKind={config.modelsByProviderKind}
            modelAliases={config.modelAliases}
            providers={config.providers}
            onMove={config.movePriority}
            onReorder={config.reorderPriority}
            onRemoveFallback={config.removeFallback}
            onMoveFallback={config.moveFallback}
            onReorderFallback={config.reorderFallback}
            onAddFallbackModel={config.addFallbackModel}
            onAddFallback={() => setShowAddFallback(true)}
            onUpdateModelAliases={config.updateModelAliases}
          />
        )}
        {view === 'logs' && (
          <LogsView
            accessKey={config.accessKey}
            logLevel={config.logLevel}
            debugDump={config.debugDump}
            isSaving={config.isSaving}
            onSaveLoggingConfig={(next) =>
              void config.persistConfig({
                logLevel: next.logLevel,
                debugDump: next.debugDump,
              })
            }
          />
        )}
      </AppShell>

      {editingProvider && (
        <ProviderModal
          provider={editingProvider}
          isEditing={Boolean(editingId)}
          allowKindChange={!editingId}
          accessKey={config.accessKey}
          onChange={setEditingProvider}
          onClose={() => setEditingProvider(null)}
          onSubmit={saveProvider}
        />
      )}
      {showAddFallback && (
        <FallbackModelModal
          allModels={config.allModels}
          fallbacks={config.fallbacks}
          onClose={() => setShowAddFallback(false)}
          onSubmit={(models) => {
            if (config.addFallbackModels(models)) setShowAddFallback(false)
          }}
        />
      )}
      {config.showApiKeyEditor && (
        <ApiKeyModal
          apiKey={config.projectApiKey}
          onClose={() => config.setShowApiKeyEditor(false)}
          onSubmit={(apiKey) => void config.persistConfig({ apiKey })}
        />
      )}
      {toast && <div className="toast">{toast}</div>}
    </>
  )
}

export default App
