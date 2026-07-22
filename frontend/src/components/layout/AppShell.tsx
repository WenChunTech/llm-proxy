import type { ReactNode } from 'react'
import { Icon } from '../Icon'
import { ThemeSwitcher } from '../ThemeSwitcher'
import { NavItem, ProviderNavGroups } from '../navigation'
import type { Provider, ProviderKindFilter, ThemeMode, View } from '../../types/domain'

const pageTitle: Record<View, string> = {
  providers: '提供商',
  routing: '模型路由',
}

export function AppShell({
  view,
  setView,
  providers,
  providerKindFilter,
  setProviderKindFilter,
  providerNavCollapsed,
  setProviderNavCollapsed,
  sidebarCollapsed,
  setSidebarCollapsed,
  themeMode,
  setThemeMode,
  isSaving,
  onOpenApiKey,
  onImportFile,
  onExport,
  children,
}: {
  view: View
  setView: (view: View) => void
  providers: Provider[]
  providerKindFilter: ProviderKindFilter
  setProviderKindFilter: (value: ProviderKindFilter) => void
  providerNavCollapsed: boolean
  setProviderNavCollapsed: (value: boolean | ((current: boolean) => boolean)) => void
  sidebarCollapsed: boolean
  setSidebarCollapsed: (value: boolean | ((current: boolean) => boolean)) => void
  themeMode: ThemeMode
  setThemeMode: (value: ThemeMode) => void
  isSaving: boolean
  onOpenApiKey: () => void
  onImportFile: (file: File) => void
  onExport: () => void
  children: ReactNode
}) {
  return (
    <div className={`app-shell ${sidebarCollapsed ? 'sidebar-is-collapsed' : ''}`}>
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">L</div>
          <div>
            <strong>LLM Proxy</strong>
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
            <span>v0.1.1</span>
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
            <button className="button button-secondary" type="button" onClick={onOpenApiKey}>
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
                  if (file) onImportFile(file)
                  event.currentTarget.value = ''
                }}
              />
            </label>
            <button className="button button-secondary" type="button" onClick={onExport}>
              <Icon name="download" size={16} />
              导出配置
            </button>
            {isSaving && <span className="saving-label">正在保存</span>}
          </div>
        </header>

        <div className="page-content">{children}</div>
      </main>
    </div>
  )
}
