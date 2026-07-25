import { useMemo, useRef } from 'react'
import { Icon } from '../../components/Icon'
import { apiAuthHeaders } from '../../lib/api'
import { copyText, downloadText } from '../../lib/browser'
import {
  filterLinesByKeyword,
  formatBytes,
  formatDumpTime,
  scrollNode,
  statusClass,
} from './format'
import { highlightKeywordInHtml, renderHighlighted } from './highlight'
import type { DumpDetail, DumpSummary } from './types'

export function DumpViewer({
  accessKey,
  items,
  selectedId,
  selectedIds,
  detail,
  fileTab,
  listFilter,
  contentFilter,
  loadingList,
  loadingDetail,
  deleting,
  listError,
  liveChunks,
  copyHint,
  onSelect,
  onToggleSelect,
  onToggleSelectAll,
  onDeleteOne,
  onContentFilterChange,
  onFileTabChange,
  onReloadDetail,
  onCopyHint,
}: {
  accessKey: string
  items: DumpSummary[]
  selectedId: string | null
  selectedIds: string[]
  detail: DumpDetail | null
  fileTab: string
  listFilter: string
  contentFilter: string
  loadingList: boolean
  loadingDetail: boolean
  deleting: boolean
  listError: string
  liveChunks: Record<string, string>
  copyHint: string
  onSelect: (id: string) => void
  onToggleSelect: (id: string) => void
  onToggleSelectAll: () => void
  onDeleteOne: (id: string) => void
  onContentFilterChange: (value: string) => void
  onFileTabChange: (value: string) => void
  onReloadDetail: (id: string) => void
  onCopyHint: (value: string) => void
}) {
  const dumpCodeRef = useRef<HTMLPreElement | null>(null)
  const selectedSet = useMemo(() => new Set(selectedIds), [selectedIds])
  const allSelected = items.length > 0 && items.every((item) => selectedSet.has(item.id))
  const someSelected = items.some((item) => selectedSet.has(item.id))

  const activeFile = useMemo(() => {
    if (!detail) return null
    return detail.files.find((file) => file.name === fileTab) ?? detail.files[0] ?? null
  }, [detail, fileTab])

  const activeContent = useMemo(() => {
    if (!activeFile) return ''
    void liveChunks
    return filterLinesByKeyword(activeFile.content, contentFilter)
  }, [activeFile, contentFilter, liveChunks])

  const activeHtml = useMemo(() => {
    if (!activeFile) return ''
    const highlighted = renderHighlighted(activeContent, activeFile.language)
    return highlightKeywordInHtml(highlighted, contentFilter)
  }, [activeContent, activeFile, contentFilter])

  async function copyActiveFile() {
    if (!activeFile) return
    try {
      await copyText(activeFile.content)
      onCopyHint('已复制到剪贴板')
      window.setTimeout(() => onCopyHint(''), 1800)
    } catch {
      onCopyHint('复制失败')
      window.setTimeout(() => onCopyHint(''), 1800)
    }
  }

  function saveActiveFile() {
    if (!detail || !activeFile) return
    downloadText(`${detail.id}_${activeFile.name}`, activeFile.content, 'text/plain;charset=utf-8')
  }

  function saveAllFiles() {
    if (!detail) return
    const parts = detail.files.map(
      (file) => `===== ${file.name} (${formatBytes(file.size)}) =====\n${file.content}\n`,
    )
    downloadText(`${detail.id}_bundle.log`, parts.join('\n'), 'text/plain;charset=utf-8')
  }

  function downloadServerFile(fileName: string) {
    if (!detail) return
    const url = `/api/debug-dumps/${encodeURIComponent(detail.id)}/files/${encodeURIComponent(fileName)}`
    void fetch(url, { headers: apiAuthHeaders(accessKey) })
      .then(async (response) => {
        if (!response.ok) throw new Error(await response.text())
        const blob = await response.blob()
        const objectUrl = URL.createObjectURL(blob)
        const link = document.createElement('a')
        link.href = objectUrl
        link.download = `${detail.id}_${fileName}`
        link.click()
        URL.revokeObjectURL(objectUrl)
      })
      .catch(() => {
        const file = detail.files.find((item) => item.name === fileName)
        if (file) downloadText(`${detail.id}_${fileName}`, file.content)
      })
  }

  return (
    <div className="dump-layout">
      <aside className="dump-list panel">
        <div className="dump-list-heading">
          <div className="dump-list-heading-left">
            <label className="dump-select-all" title={allSelected ? '取消全选' : '全选当前列表'}>
              <input
                type="checkbox"
                checked={allSelected}
                ref={(node) => {
                  if (node) node.indeterminate = !allSelected && someSelected
                }}
                onChange={onToggleSelectAll}
                disabled={!items.length || deleting}
              />
              <strong>会话列表</strong>
            </label>
          </div>
          <span>
            {selectedIds.length > 0 ? `${selectedIds.length}/` : ''}
            {items.length}
          </span>
        </div>
        {loadingList && <div className="dump-empty">加载中…</div>}
        {!loadingList && listError && <div className="dump-empty is-error">{listError}</div>}
        {!loadingList && !listError && !items.length && (
          <div className="dump-empty">
            {listFilter.trim()
              ? '没有匹配的请求转储。可尝试其他关键字（模型 / 请求体 / 响应体）。'
              : '暂无请求转储。发起一次代理请求后会出现在这里。'}
          </div>
        )}
        <div className="dump-list-scroll">
          {items.map((item) => {
            const checked = selectedSet.has(item.id)
            return (
              <div
                key={item.id}
                className={`dump-list-item ${selectedId === item.id ? 'active' : ''} ${checked ? 'is-checked' : ''}`}
              >
                <label className="dump-list-check" onClick={(event) => event.stopPropagation()}>
                  <input
                    type="checkbox"
                    checked={checked}
                    disabled={deleting}
                    onChange={() => onToggleSelect(item.id)}
                  />
                </label>
                <button
                  type="button"
                  className="dump-list-item-main"
                  onClick={() => onSelect(item.id)}
                >
                  <div className="dump-list-item-top">
                    <strong title={item.model}>{item.model || 'unknown model'}</strong>
                    <span className={`dump-status ${statusClass(item.status)}`}>
                      {item.status ?? '…'}
                    </span>
                  </div>
                  <div className="dump-list-item-meta">
                    <span>{item.provider || item.endpoint || '—'}</span>
                    <span>{item.is_streaming ? 'stream' : 'json'}</span>
                  </div>
                  <div className="dump-list-item-id">{formatDumpTime(item.id, item.mtime_ms)}</div>
                  {item.matches && item.matches.length > 0 && (
                    <div className="dump-list-item-matches" title={item.matches.join(', ')}>
                      匹配：{item.matches.slice(0, 4).join(' · ')}
                      {item.matches.length > 4 ? ' …' : ''}
                    </div>
                  )}
                </button>
                <button
                  type="button"
                  className="icon-button subtle danger-button dump-list-delete"
                  title="删除该会话"
                  disabled={deleting}
                  onClick={(event) => {
                    event.stopPropagation()
                    onDeleteOne(item.id)
                  }}
                >
                  <Icon name="trash" size={14} />
                </button>
              </div>
            )
          })}
        </div>
      </aside>

      <div className="dump-detail panel">
        {!selectedId && <div className="dump-empty">选择左侧会话查看请求/响应体</div>}
        {selectedId && loadingDetail && !detail && <div className="dump-empty">加载内容…</div>}
        {selectedId && detail && (
          <>
            <div className="dump-detail-header">
              <div>
                <div className="dump-detail-title">
                  <h3>{detail.model || detail.id}</h3>
                  <span className={`dump-status ${statusClass(detail.status)}`}>
                    {detail.status ?? 'pending'}
                  </span>
                </div>
                <div className="dump-detail-sub">
                  <span>{detail.provider}</span>
                  <span className="footer-dot">•</span>
                  <span>{detail.endpoint}</span>
                  <span className="footer-dot">•</span>
                  <span>{detail.is_streaming ? 'streaming' : 'non-stream'}</span>
                  <span className="footer-dot">•</span>
                  <span className="mono">{detail.id}</span>
                </div>
              </div>
              <div className="logs-actions">
                <button
                  className="button button-secondary"
                  type="button"
                  onClick={() => onReloadDetail(detail.id)}
                >
                  重新加载
                </button>
                <button
                  className="button button-secondary danger-action"
                  type="button"
                  disabled={deleting}
                  onClick={() => onDeleteOne(detail.id)}
                >
                  <Icon name="trash" size={15} />
                  删除会话
                </button>
                <button
                  className="button button-secondary"
                  type="button"
                  disabled={!activeFile}
                  onClick={() => void copyActiveFile()}
                >
                  <Icon name="copy" size={15} />
                  复制内容
                </button>
                <button
                  className="button button-secondary"
                  type="button"
                  disabled={!activeFile}
                  onClick={saveActiveFile}
                >
                  <Icon name="download" size={15} />
                  保存当前
                </button>
                <button className="button button-primary" type="button" onClick={saveAllFiles}>
                  <Icon name="download" size={15} />
                  保存全部
                </button>
              </div>
            </div>

            <div className="dump-file-tabs">
              {detail.files.map((file) => (
                <button
                  key={file.name}
                  type="button"
                  className={fileTab === file.name ? 'active' : ''}
                  onClick={() => onFileTabChange(file.name)}
                >
                  {file.name}
                  <small>{formatBytes(file.size)}</small>
                </button>
              ))}
            </div>

            {activeFile ? (
              <div className="dump-file-pane">
                <div className="dump-file-meta">
                  <div className="dump-file-meta-left">
                    <span>
                      {activeFile.name}
                      {activeFile.truncated ? '（内容已截断，可下载完整文件）' : ''}
                      {contentFilter.trim() ? ' · 关键字过滤中' : ''}
                      {copyHint ? ` · ${copyHint}` : ''}
                    </span>
                    <label className="search-field logs-filter dump-content-filter">
                      <Icon name="search" size={14} />
                      <input
                        value={contentFilter}
                        onChange={(event) => onContentFilterChange(event.target.value)}
                        placeholder="关键字搜索文件内容"
                      />
                    </label>
                  </div>
                  <div className="logs-actions">
                    <button
                      className="text-button"
                      type="button"
                      onClick={() => downloadServerFile(activeFile.name)}
                    >
                      下载原始文件
                    </button>
                  </div>
                </div>
                <div className="logs-scroll-shell dump-code-shell">
                  <pre
                    ref={dumpCodeRef}
                    className={`dump-code language-${activeFile.language}`}
                    dangerouslySetInnerHTML={{
                      __html: activeHtml || (contentFilter.trim() ? '无匹配内容' : ' '),
                    }}
                  />
                  <div className="logs-scroll-actions" aria-label="滚动控制">
                    <button
                      className="button button-secondary logs-scroll-button"
                      type="button"
                      title="置顶"
                      onClick={() => {
                        requestAnimationFrame(() => scrollNode(dumpCodeRef.current, 'top'))
                      }}
                    >
                      <Icon name="toTop" size={14} />
                      置顶
                    </button>
                    <button
                      className="button button-secondary logs-scroll-button"
                      type="button"
                      title="置底"
                      onClick={() => {
                        requestAnimationFrame(() => scrollNode(dumpCodeRef.current, 'bottom'))
                      }}
                    >
                      <Icon name="toBottom" size={14} />
                      置底
                    </button>
                  </div>
                </div>
              </div>
            ) : (
              <div className="dump-empty">该会话暂无文件</div>
            )}
          </>
        )}
      </div>
    </div>
  )
}
