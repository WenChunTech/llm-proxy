import { useEffect, useRef, useState } from 'react'
import type { CSSProperties, PointerEvent as ReactPointerEvent, ReactNode } from 'react'
import { Icon } from '../Icon'
import { LanguageSwitcher } from '../LanguageSwitcher'
import { ThemeSwitcher } from '../ThemeSwitcher'
import { NavItem, ProviderNavGroups } from '../navigation'
import { useI18n } from '../../lib/i18n'
import {
  SIDEBAR_WIDTH_DEFAULT,
  SIDEBAR_WIDTH_MAX,
  SIDEBAR_WIDTH_MIN,
  persistSidebarWidth,
  readStoredSidebarWidth,
} from '../../lib/storage'
import type { Provider, ProviderKindFilter, ThemeMode, View } from '../../types/domain'

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
  const { t } = useI18n()
  const [sidebarWidth, setSidebarWidth] = useState(() => readStoredSidebarWidth())
  const [isResizing, setIsResizing] = useState(false)
  const resizeOrigin = useRef<{ startX: number; startWidth: number } | null>(null)
  const pageTitle: Record<View, string> = {
    providers: t('shell.pageTitle.providers'),
    routing: t('shell.pageTitle.routing'),
    logs: t('shell.pageTitle.logs'),
  }

  useEffect(() => {
    if (!isResizing) return
    document.body.classList.add('is-resizing-sidebar')
    return () => document.body.classList.remove('is-resizing-sidebar')
  }, [isResizing])

  function handleResizeStart(event: ReactPointerEvent<HTMLButtonElement>) {
    if (sidebarCollapsed) return
    resizeOrigin.current = { startX: event.clientX, startWidth: sidebarWidth }
    setIsResizing(true)
    event.currentTarget.setPointerCapture(event.pointerId)
    event.currentTarget.classList.add('is-dragging')
  }

  function handleResizeMove(event: ReactPointerEvent<HTMLButtonElement>) {
    const origin = resizeOrigin.current
    if (!origin) return
    const maxWidth = Math.max(SIDEBAR_WIDTH_MIN, Math.min(SIDEBAR_WIDTH_MAX, window.innerWidth - 340))
    const nextWidth = Math.min(maxWidth, Math.max(SIDEBAR_WIDTH_MIN, origin.startWidth + event.clientX - origin.startX))
    setSidebarWidth(nextWidth)
  }

  function handleResizeStop(event: ReactPointerEvent<HTMLButtonElement>) {
    if (!resizeOrigin.current) return
    resizeOrigin.current = null
    setIsResizing(false)
    event.currentTarget.classList.remove('is-dragging')
    setSidebarWidth((width) => persistSidebarWidth(width))
  }

  function handleResizeReset() {
    setSidebarWidth(persistSidebarWidth(SIDEBAR_WIDTH_DEFAULT))
  }

  return (
    <div
      className={`app-shell ${sidebarCollapsed ? 'sidebar-is-collapsed' : ''} ${isResizing ? 'is-resizing-sidebar' : ''}`}
      style={{ '--sidebar-width': `${sidebarWidth}px` } as CSSProperties}
    >
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">L</div>
          <div>
            <strong>LLM Proxy</strong>
            <span>{t('shell.controlCenter')}</span>
          </div>
          <button
            className="sidebar-collapse-button"
            type="button"
            title={sidebarCollapsed ? t('shell.expandSidebar') : t('shell.collapseSidebar')}
            aria-label={sidebarCollapsed ? t('shell.expandSidebar') : t('shell.collapseSidebar')}
            onClick={() => setSidebarCollapsed((collapsed) => !collapsed)}
          >
            <Icon name="sidebar" size={17} />
          </button>
        </div>

        <nav className="nav-list" aria-label={t('shell.mainNav')}>
          <NavItem
            icon="server"
            label={t('shell.pageTitle.providers')}
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
            label={t('shell.pageTitle.routing')}
            active={view === 'routing'}
            onClick={() => setView('routing')}
          />
          <NavItem
            icon="terminal"
            label={t('shell.pageTitle.logs')}
            active={view === 'logs'}
            onClick={() => setView('logs')}
          />
        </nav>

        <div className="sidebar-bottom">
          <div className="sidebar-footer">
            <span>v0.1.17</span>
            <span className="footer-dot">•</span>
            <span>{t('shell.rustRuntime')}</span>
          </div>
        </div>

        <button
          className="sidebar-resize-handle"
          type="button"
          role="separator"
          aria-orientation="vertical"
          aria-label={t('shell.resizeSidebar')}
          title={t('shell.resizeSidebar')}
          aria-valuemin={SIDEBAR_WIDTH_MIN}
          aria-valuemax={SIDEBAR_WIDTH_MAX}
          aria-valuenow={sidebarWidth}
          onPointerDown={handleResizeStart}
          onPointerMove={handleResizeMove}
          onPointerUp={handleResizeStop}
          onPointerCancel={handleResizeStop}
          onDoubleClick={handleResizeReset}
        />
      </aside>

      <main className="main-content">
        <header className="topbar">
          <div>
            <div className="breadcrumb">
              <span>{t('shell.embeddedConsole')}</span>
              <Icon name="chevron" size={13} />
              <strong>{pageTitle[view]}</strong>
            </div>
            <h1>{pageTitle[view]}</h1>
          </div>
          <div className="topbar-actions">
            <LanguageSwitcher />
            <ThemeSwitcher value={themeMode} onChange={setThemeMode} />
            <button className="button button-secondary" type="button" onClick={onOpenApiKey}>
              <Icon name="key" size={16} />
              API Key
            </button>
            <label className="button button-secondary import-button">
              <Icon name="upload" size={16} />
              {t('shell.importConfig')}
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
              {t('shell.exportConfig')}
            </button>
            {isSaving && <span className="saving-label">{t('shell.saving')}</span>}
          </div>
        </header>

        <div className="page-content">{children}</div>
      </main>
    </div>
  )
}
