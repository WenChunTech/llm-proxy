import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { Icon } from '../../components/Icon'
import { apiAuthHeaders } from '../../lib/api'
import { downloadText } from '../../lib/browser'
import type { DebugDumpConfig } from '../../types/domain'
import { DumpViewer } from './DumpViewer'
import { LoggingSettings } from './LoggingSettings'
import { ProcessLogsPanel, type ProcessLogsPanelHandle } from './ProcessLogsPanel'
import {
  connectionLabel,
  normalizeLogLevel,
  preferFile,
  type LogLevelPreset,
} from './format'
import type { DumpDetail, DumpListPayload, DumpSummary, PageTab } from './types'
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
  const [detail, setDetail] = useState<DumpDetail | null>(null)
  const [fileTab, setFileTab] = useState('')
  const [listFilter, setListFilter] = useState('')
  const [contentFilter, setContentFilter] = useState('')
  const [loadingList, setLoadingList] = useState(false)
  const [loadingDetail, setLoadingDetail] = useState(false)
  const [listError, setListError] = useState('')
  const [liveChunks, setLiveChunks] = useState<Record<string, string>>({})
  const [processFilter, setProcessFilter] = useState('')
  const [autoScroll, setAutoScroll] = useState(true)
  const [copyHint, setCopyHint] = useState('')
  const selectedIdRef = useRef<string | null>(null)
  selectedIdRef.current = selectedId
  const processPanelRef = useRef<ProcessLogsPanelHandle | null>(null)

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

  const loadList = useCallback(async () => {
    if (!dumpEnabled) {
      setItems([])
      setSelectedId(null)
      setDetail(null)
      return
    }
    setLoadingList(true)
    setListError('')
    try {
      const response = await fetch('/api/debug-dumps', {
        headers: apiAuthHeaders(accessKey),
        signal: AbortSignal.timeout(15000),
      })
      if (!response.ok) throw new Error(await response.text())
      const payload = (await response.json()) as DumpListPayload
      setDumpsDir(payload.dir || 'logs')
      const nextItems = Array.isArray(payload.items) ? payload.items : []
      setItems(nextItems)
      setSelectedId((current) => {
        if (current && nextItems.some((item) => item.id === current)) return current
        return nextItems[0]?.id ?? null
      })
    } catch (error) {
      setListError(error instanceof Error ? error.message : '加载失败')
    } finally {
      setLoadingList(false)
    }
  }, [accessKey, dumpEnabled])

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
          const names = payload.files.map((file) => file.name)
          if (current && names.includes(current)) return current
          return preferFile(names)
        })
      } catch {
        setDetail(null)
      } finally {
        setLoadingDetail(false)
      }
    },
    [accessKey],
  )

  useEffect(() => {
    void loadList()
  }, [loadList])

  useEffect(() => {
    if (!dumpEnabled || !selectedId) {
      setDetail(null)
      return
    }
    void loadDetail(selectedId)
  }, [selectedId, loadDetail, dumpEnabled])

  const { connection, processLines, setProcessLines } = useLogsSocket({
    accessKey,
    dumpEnabled,
    onDumpsDir: setDumpsDir,
    onDumpEvent: ({ type, summary }) => {
      setItems((current) => {
        const rest = current.filter((item) => item.id !== summary.id)
        return [summary, ...rest].slice(0, 500)
      })
      if (type === 'created' && !selectedIdRef.current) setSelectedId(summary.id)
      if (selectedIdRef.current === summary.id && type === 'updated') {
        void loadDetail(summary.id)
      }
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

  return (
    <section className="logs-page">
      <section className="page-intro">
        <div>
          <span className="eyebrow">Runtime</span>
          <h2>请求日志</h2>
          <p>
            {dumpEnabled
              ? '查看请求/响应转储与进程输出'
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
        </div>

        <div className="logs-actions">
          {pageTab === 'dumps' && dumpEnabled && (
            <>
              <label className="search-field logs-filter">
                <Icon name="search" size={15} />
                <input
                  value={listFilter}
                  onChange={(event) => setListFilter(event.target.value)}
                  placeholder="搜索会话：模型 / 提供商 / ID"
                />
              </label>
              <button className="button button-secondary" type="button" onClick={() => void loadList()}>
                刷新列表
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
                title="置顶"
                onClick={() => {
                  setAutoScroll(false)
                  processPanelRef.current?.scrollTo('top')
                }}
              >
                <Icon name="toTop" size={15} />
                置顶
              </button>
              <button
                className="button button-secondary"
                type="button"
                title="置底"
                onClick={() => {
                  setAutoScroll(true)
                  processPanelRef.current?.scrollTo('bottom')
                }}
              >
                <Icon name="toBottom" size={15} />
                置底
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
          detail={detail}
          fileTab={fileTab}
          listFilter={listFilter}
          contentFilter={contentFilter}
          loadingList={loadingList}
          loadingDetail={loadingDetail}
          listError={listError}
          liveChunks={liveChunks}
          copyHint={copyHint}
          onSelect={setSelectedId}
          onContentFilterChange={setContentFilter}
          onFileTabChange={setFileTab}
          onReloadDetail={(id) => void loadDetail(id)}
          onCopyHint={setCopyHint}
        />
      ) : pageTab === 'process' ? (
        <ProcessLogsPanel ref={processPanelRef} lines={processLines} filter={processFilter} autoScroll={autoScroll} />
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
