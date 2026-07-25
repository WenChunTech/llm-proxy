import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Icon } from '../../components/Icon'
import { apiAuthHeaders } from '../../lib/api'
import { downloadText } from '../../lib/browser'
import type { DebugDumpConfig } from '../../types/domain'
import { DumpViewer } from './DumpViewer'
import { LoggingSettings } from './LoggingSettings'
import { ProcessLogsPanel } from './ProcessLogsPanel'
import {
  connectionLabel,
  normalizeLogLevel,
  preferFile,
  type LogLevelPreset,
} from './format'
import type { DumpDeletePayload, DumpDetail, DumpListPayload, DumpSummary, PageTab } from './types'
import { appendLiveChunk, useLogsSocket } from './useLogsSocket'
import './logs.css'

export function LogsView({
  accessKey,
  logLevel,
  debugDump,
  isSaving = false,
  onSaveLoggingConfig,
}: {
  accessKey: string
  logLevel: string
  debugDump: DebugDumpConfig
  isSaving?: boolean
  onSaveLoggingConfig: (next: {
    logLevel: string
    debugDump: DebugDumpConfig
  }) => void | Promise<void>
}) {
  const dumpEnabled = Boolean(debugDump?.enabled)
  const [pageTab, setPageTab] = useState<PageTab>(dumpEnabled ? 'dumps' : 'process')
  const [draftLogLevel, setDraftLogLevel] = useState<LogLevelPreset>(normalizeLogLevel(logLevel))
  const [draftDebugDump, setDraftDebugDump] = useState<DebugDumpConfig>({
    enabled: Boolean(debugDump?.enabled),
    dir: debugDump?.dir?.trim() || 'logs',
  })
  const [dumpsDir, setDumpsDir] = useState(debugDump?.dir?.trim() || 'logs')
  const [items, setItems] = useState<DumpSummary[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [selectedIds, setSelectedIds] = useState<string[]>([])
  const [detail, setDetail] = useState<DumpDetail | null>(null)
  const [fileTab, setFileTab] = useState('')
  const [listFilter, setListFilter] = useState('')
  const [debouncedFilter, setDebouncedFilter] = useState('')
  const [contentFilter, setContentFilter] = useState('')
  const [loadingList, setLoadingList] = useState(false)
  const [loadingDetail, setLoadingDetail] = useState(false)
  const [deleting, setDeleting] = useState(false)
  const [listError, setListError] = useState('')
  const [actionHint, setActionHint] = useState('')
  const [liveChunks, setLiveChunks] = useState<Record<string, string>>({})
  const [processFilter, setProcessFilter] = useState('')
  const [autoScroll, setAutoScroll] = useState(true)
  const [copyHint, setCopyHint] = useState('')
  const selectedIdRef = useRef<string | null>(null)
  selectedIdRef.current = selectedId
  const listFilterRef = useRef('')
  listFilterRef.current = listFilter
  const reloadTimerRef = useRef<number | null>(null)

  useEffect(() => {
    setDraftLogLevel(normalizeLogLevel(logLevel))
    setDraftDebugDump({
      enabled: Boolean(debugDump?.enabled),
      dir: debugDump?.dir?.trim() || 'logs',
    })
    setDumpsDir(debugDump?.dir?.trim() || 'logs')
  }, [logLevel, debugDump])

  useEffect(() => {
    if (!dumpEnabled && pageTab === 'dumps') {
      setPageTab('process')
    }
  }, [dumpEnabled, pageTab])

  useEffect(() => {
    const timer = window.setTimeout(() => {
      setDebouncedFilter(listFilter.trim())
    }, 280)
    return () => window.clearTimeout(timer)
  }, [listFilter])

  const loggingDirty = useMemo(() => {
    const nextLevel = draftLogLevel
    const currentLevel = normalizeLogLevel(logLevel)
    const nextDir = draftDebugDump.dir.trim() || 'logs'
    const currentDir = (debugDump?.dir || 'logs').trim() || 'logs'
    return (
      nextLevel !== currentLevel ||
      Boolean(draftDebugDump.enabled) !== Boolean(debugDump?.enabled) ||
      nextDir !== currentDir
    )
  }, [debugDump, draftDebugDump, draftLogLevel, logLevel])

  function saveLoggingConfig() {
    const nextDir = draftDebugDump.dir.trim() || 'logs'
    void onSaveLoggingConfig({
      logLevel: draftLogLevel,
      debugDump: {
        enabled: Boolean(draftDebugDump.enabled),
        dir: nextDir,
      },
    })
  }

  const showActionHint = useCallback((message: string) => {
    setActionHint(message)
    window.setTimeout(() => {
      setActionHint((current) => (current === message ? '' : current))
    }, 2200)
  }, [])

  const loadList = useCallback(async (query = debouncedFilter) => {
    if (!dumpEnabled) {
      setItems([])
      setSelectedId(null)
      setSelectedIds([])
      setDetail(null)
      return
    }
    setLoadingList(true)
    setListError('')
    try {
      const params = new URLSearchParams()
      const trimmed = query.trim()
      if (trimmed) params.set('q', trimmed)
      const url = params.size ? `/api/debug-dumps?${params}` : '/api/debug-dumps'
      const response = await fetch(url, {
        headers: apiAuthHeaders(accessKey),
        signal: AbortSignal.timeout(20000),
      })
      if (!response.ok) throw new Error(await response.text())
      const payload = (await response.json()) as DumpListPayload
      setDumpsDir(payload.dir || 'logs')
      const nextItems = Array.isArray(payload.items) ? payload.items : []
      setItems(nextItems)
      setSelectedIds((current) => current.filter((id) => nextItems.some((item) => item.id === id)))
      setSelectedId((current) => {
        if (current && nextItems.some((item) => item.id === current)) return current
        return nextItems[0]?.id ?? null
      })
    } catch (error) {
      setListError(error instanceof Error ? error.message : '加载失败')
    } finally {
      setLoadingList(false)
    }
  }, [accessKey, debouncedFilter, dumpEnabled])

  const scheduleReloadList = useCallback(() => {
    if (reloadTimerRef.current !== null) {
      window.clearTimeout(reloadTimerRef.current)
    }
    reloadTimerRef.current = window.setTimeout(() => {
      void loadList(listFilterRef.current)
    }, 250)
  }, [loadList])

  const loadDetail = useCallback(
    async (id: string) => {
      setLoadingDetail(true)
      try {
        const response = await fetch(`/api/debug-dumps/${encodeURIComponent(id)}`, {
          headers: apiAuthHeaders(accessKey),
          signal: AbortSignal.timeout(15000),
        })
        if (!response.ok) throw new Error(await response.text())
        const payload = (await response.json()) as DumpDetail
        setDetail(payload)
        setFileTab((current) => {
          if (current && payload.files.some((file) => file.name === current)) return current
          return preferFile(payload.files.map((file) => file.name))
        })
        setLiveChunks((current) => {
          if (!(id in current)) return current
          const next = { ...current }
          delete next[id]
          return next
        })
      } catch (error) {
        setDetail(null)
        setListError(error instanceof Error ? error.message : '加载详情失败')
      } finally {
        setLoadingDetail(false)
      }
    },
    [accessKey],
  )

  useEffect(() => {
    void loadList(debouncedFilter)
  }, [loadList, debouncedFilter])

  useEffect(() => {
    if (!dumpEnabled || !selectedId) {
      setDetail(null)
      return
    }
    void loadDetail(selectedId)
  }, [selectedId, loadDetail, dumpEnabled])

  useEffect(() => {
    return () => {
      if (reloadTimerRef.current !== null) {
        window.clearTimeout(reloadTimerRef.current)
      }
    }
  }, [])

  const removeLocalDumps = useCallback((ids: string[]) => {
    if (!ids.length) return
    const idSet = new Set(ids)
    setItems((current) => {
      const nextItems = current.filter((item) => !idSet.has(item.id))
      setSelectedId((selected) => {
        if (selected && nextItems.some((item) => item.id === selected)) return selected
        return nextItems[0]?.id ?? null
      })
      setDetail((currentDetail) => {
        if (!currentDetail || !idSet.has(currentDetail.id)) return currentDetail
        return null
      })
      return nextItems
    })
    setSelectedIds((current) => current.filter((id) => !idSet.has(id)))
    setLiveChunks((current) => {
      let changed = false
      const next = { ...current }
      for (const id of ids) {
        if (id in next) {
          delete next[id]
          changed = true
        }
      }
      return changed ? next : current
    })
  }, [])

  const deleteDumps = useCallback(
    async (ids: string[], mode: 'one' | 'selected' | 'filtered') => {
      const uniqueIds = Array.from(new Set(ids.map((id) => id.trim()).filter(Boolean)))
      if (!uniqueIds.length || deleting) return

      const labels = {
        one: `确定删除该转储会话吗？\n${uniqueIds[0]}`,
        selected: `确定删除选中的 ${uniqueIds.length} 个转储会话吗？`,
        filtered: `确定删除当前筛选结果中的 ${uniqueIds.length} 个转储会话吗？`,
      }
      if (!window.confirm(labels[mode])) return

      setDeleting(true)
      setListError('')
      try {
        const response =
          uniqueIds.length === 1
            ? await fetch(`/api/debug-dumps/${encodeURIComponent(uniqueIds[0])}`, {
                method: 'DELETE',
                headers: apiAuthHeaders(accessKey),
                signal: AbortSignal.timeout(15000),
              })
            : await fetch('/api/debug-dumps/delete', {
                method: 'POST',
                headers: apiAuthHeaders(accessKey, {
                  'content-type': 'application/json',
                }),
                body: JSON.stringify({ ids: uniqueIds }),
                signal: AbortSignal.timeout(30000),
              })
        if (!response.ok) throw new Error(await response.text())
        const payload = (await response.json()) as DumpDeletePayload
        const deleted = Array.isArray(payload.deleted) ? payload.deleted : []
        const failed = Array.isArray(payload.failed) ? payload.failed : []
        removeLocalDumps(deleted)
        if (failed.length) {
          setListError(`部分删除失败：${failed.map((item) => item.id || item.error || 'unknown').join(', ')}`)
          scheduleReloadList()
        } else {
          showActionHint(`已删除 ${deleted.length} 个会话`)
        }
      } catch (error) {
        setListError(error instanceof Error ? error.message : '删除失败')
      } finally {
        setDeleting(false)
      }
    },
    [accessKey, deleting, removeLocalDumps, scheduleReloadList, showActionHint],
  )

  const { connection, processLines, setProcessLines } = useLogsSocket({
    accessKey,
    dumpEnabled,
    onDumpsDir: setDumpsDir,
    onDumpEvent: ({ type, summary }) => {
      if (listFilterRef.current.trim()) {
        scheduleReloadList()
        if (type === 'created' && !selectedIdRef.current) setSelectedId(summary.id)
        return
      }
      setItems((current) => {
        const rest = current.filter((item) => item.id !== summary.id)
        return [summary, ...rest].slice(0, 500)
      })
      if (type === 'created' && !selectedIdRef.current) setSelectedId(summary.id)
      if (selectedIdRef.current === summary.id && type === 'updated') {
        void loadDetail(summary.id)
      }
    },
    onDumpDeleted: ({ id }) => {
      removeLocalDumps([id])
    },
    onChunk: ({ id, text }) => {
      setLiveChunks((current) => appendLiveChunk(current, id, text))
      if (selectedIdRef.current === id) {
        setDetail((current) => {
          if (!current || current.id !== id) return current
          const files = current.files.map((file) => {
            if (file.name !== 'response.sse') return file
            return {
              ...file,
              content: `${file.content}${text}`,
              size: file.size + text.length,
            }
          })
          if (!files.some((file) => file.name === 'response.sse')) {
            files.push({
              name: 'response.sse',
              size: text.length,
              truncated: false,
              content: text,
              language: 'sse',
            })
          }
          return { ...current, files }
        })
      }
    },
  })

  function toggleSelect(id: string) {
    setSelectedIds((current) =>
      current.includes(id) ? current.filter((item) => item !== id) : [...current, id],
    )
  }

  function toggleSelectAll() {
    setSelectedIds((current) => {
      if (items.length && items.every((item) => current.includes(item.id))) {
        return []
      }
      return items.map((item) => item.id)
    })
  }

  return (
    <section className="logs-page">
      <section className="page-intro">
        <div>
          <span className="eyebrow">Runtime</span>
          <h2>请求日志</h2>
          <p>
            {dumpEnabled
              ? '查看、搜索并管理请求/响应转储与进程输出'
              : '查看实时进程输出 可在日志配置中调整等级或启用请求转储'}
          </p>
        </div>
        {dumpEnabled ? (
          <div className="logs-dir-badge" title={`debug_dump 目录：${dumpsDir}`}>
            <span className="status-dot" />
            <span>转储目录</span>
            <code>{dumpsDir}</code>
          </div>
        ) : (
          <div className="logs-dir-badge is-muted">
            <span className="status-dot" />
            <span>请求转储未启用</span>
          </div>
        )}
      </section>

      <div className="logs-toolbar">
        <div className="segmented-control">
          {dumpEnabled && (
            <button
              type="button"
              className={pageTab === 'dumps' ? 'selected' : ''}
              onClick={() => setPageTab('dumps')}
            >
              请求转储
              <span>{items.length}</span>
            </button>
          )}
          <button
            type="button"
            className={pageTab === 'process' ? 'selected' : ''}
            onClick={() => setPageTab('process')}
          >
            进程日志
            <span>{processLines.length}</span>
          </button>
          <button
            type="button"
            className={pageTab === 'settings' ? 'selected' : ''}
            onClick={() => setPageTab('settings')}
          >
            日志配置
          </button>
        </div>

        <div className="logs-status-group">
          <span className={`logs-status-dot ${connection}`} />
          <strong>{connectionLabel(connection)}</strong>
          {actionHint ? <span className="logs-meta">{actionHint}</span> : null}
        </div>

        <div className="logs-actions">
          {pageTab === 'dumps' && dumpEnabled && (
            <>
              <label className="search-field logs-filter logs-filter-wide">
                <Icon name="search" size={15} />
                <input
                  value={listFilter}
                  onChange={(event) => setListFilter(event.target.value)}
                  placeholder="搜索：模型 / 提供商 / 请求体 / 响应体"
                />
              </label>
              <button
                className="button button-secondary"
                type="button"
                disabled={loadingList || deleting}
                onClick={() => void loadList(listFilter)}
              >
                刷新列表
              </button>
              <button
                className="button button-secondary danger-action"
                type="button"
                disabled={!selectedIds.length || deleting}
                onClick={() => void deleteDumps(selectedIds, 'selected')}
              >
                <Icon name="trash" size={15} />
                删除选中
                {selectedIds.length ? ` (${selectedIds.length})` : ''}
              </button>
              <button
                className="button button-secondary danger-action"
                type="button"
                disabled={!items.length || deleting}
                onClick={() => void deleteDumps(items.map((item) => item.id), 'filtered')}
              >
                <Icon name="trash" size={15} />
                删除筛选结果
                {items.length ? ` (${items.length})` : ''}
              </button>
            </>
          )}
          {pageTab === 'process' && (
            <>
              <label className="search-field logs-filter">
                <Icon name="search" size={15} />
                <input
                  value={processFilter}
                  onChange={(event) => setProcessFilter(event.target.value)}
                  placeholder="关键字搜索进程日志"
                />
              </label>
              <button
                className={`button button-secondary ${autoScroll ? 'is-active' : ''}`}
                type="button"
                onClick={() => setAutoScroll((value) => !value)}
              >
                自动滚动
              </button>
              <button
                className="button button-secondary"
                type="button"
                onClick={() => setProcessLines([])}
              >
                清空
              </button>
              <button
                className="button button-primary"
                type="button"
                disabled={!processLines.length}
                onClick={() =>
                  downloadText(
                    `process-${new Date().toISOString().replaceAll(':', '').slice(0, 15)}.log`,
                    `${processLines.join('\n')}\n`,
                  )
                }
              >
                <Icon name="download" size={15} />
                保存
              </button>
            </>
          )}
        </div>
      </div>

      {pageTab === 'dumps' && dumpEnabled ? (
        <DumpViewer
          accessKey={accessKey}
          items={items}
          selectedId={selectedId}
          selectedIds={selectedIds}
          detail={detail}
          fileTab={fileTab}
          listFilter={listFilter}
          contentFilter={contentFilter}
          loadingList={loadingList}
          loadingDetail={loadingDetail}
          deleting={deleting}
          listError={listError}
          liveChunks={liveChunks}
          copyHint={copyHint}
          onSelect={setSelectedId}
          onToggleSelect={toggleSelect}
          onToggleSelectAll={toggleSelectAll}
          onDeleteOne={(id) => void deleteDumps([id], 'one')}
          onContentFilterChange={setContentFilter}
          onFileTabChange={setFileTab}
          onReloadDetail={(id) => void loadDetail(id)}
          onCopyHint={setCopyHint}
        />
      ) : pageTab === 'process' ? (
        <ProcessLogsPanel
          lines={processLines}
          filter={processFilter}
          autoScroll={autoScroll}
          onAutoScrollChange={setAutoScroll}
        />
      ) : (
        <LoggingSettings
          draftLogLevel={draftLogLevel}
          draftDebugDump={draftDebugDump}
          loggingDirty={loggingDirty}
          isSaving={isSaving}
          onLogLevelChange={setDraftLogLevel}
          onDebugDumpChange={setDraftDebugDump}
          onReset={() => {
            setDraftLogLevel(normalizeLogLevel(logLevel))
            setDraftDebugDump({
              enabled: Boolean(debugDump?.enabled),
              dir: debugDump?.dir?.trim() || 'logs',
            })
          }}
          onSave={saveLoggingConfig}
        />
      )}
    </section>
  )
}
