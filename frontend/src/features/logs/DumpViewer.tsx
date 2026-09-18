import { useMemo, useRef } from 'react'
import { Icon } from '../../components/Icon'
import { useI18n } from '../../lib/i18n'
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
  const { t } = useI18n()
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
      onCopyHint(t('dump.copiedToClipboard'))
      window.setTimeout(() => onCopyHint(''), 1800)
    } catch {
      onCopyHint(t('dump.copyFailed'))
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
            <label className="dump-select-all" title={allSelected ? t('dump.deselectAll') : t('dump.selectAll')}>
              <input
                type="checkbox"
                checked={allSelected}
                ref={(node) => {
                  if (node) node.indeterminate = !allSelected && someSelected
                }}
                onChange={onToggleSelectAll}
                disabled={!items.length || deleting}
              />
              <strong>{t('dump.sessionList')}</strong>
            </label>
          </div>
          <span>
            {selectedIds.length > 0 ? `${selectedIds.length}/` : ''}
            {items.length}
          </span>
        </div>
        {loadingList && <div className="dump-empty">{t('dump.loading')}</div>}
        {!loadingList && listError && <div className="dump-empty is-error">{listError}</div>}
        {!loadingList && !listError && !items.length && (
          <div className="dump-empty">
            {listFilter.trim()
              ? t('dump.noMatchFiltered')
              : t('dump.noDumps')}
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
                      {t('dump.matches')} {item.matches.slice(0, 4).join(' · ')}
                      {item.matches.length > 4 ? ' …' : ''}
                    </div>
                  )}
                </button>
                <button
                  type="button"
                  className="icon-button subtle danger-button dump-list-delete"
                  title={t('dump.deleteSession')}
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
        {!selectedId && <div className="dump-empty">{t('dump.selectLeftHint')}</div>}
        {selectedId && loadingDetail && !detail && <div className="dump-empty">{t('dump.loadingContent')}</div>}
        {selectedId && detail && (
          <>
            <div className="dump-detail-header">
              <div>
                <div className="dump-detail-title">
                  <h3>{detail.model || detail.id}</h3>
                  <span className={`dump-status ${statusClass(detail.status)}`}>
                    {detail.status ?? t('dump.pending')}
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
                  {t('dump.reload')}
                </button>
                <button
                  className="button button-secondary danger-action"
                  type="button"
                  disabled={deleting}
                  onClick={() => onDeleteOne(detail.id)}
                >
                  <Icon name="trash" size={15} />
                  {t('dump.deleteSessionBtn')}
                </button>
                <button
                  className="button button-secondary"
                  type="button"
                  disabled={!activeFile}
                  onClick={() => void copyActiveFile()}
                >
                  <Icon name="copy" size={15} />
                  {t('dump.copyContent')}
                </button>
                <button
                  className="button button-secondary"
                  type="button"
                  disabled={!activeFile}
                  onClick={saveActiveFile}
                >
                  <Icon name="download" size={15} />
                  {t('dump.saveCurrent')}
                </button>
                <button className="button button-primary" type="button" onClick={saveAllFiles}>
                  <Icon name="download" size={15} />
                  {t('dump.saveAll')}
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
                      {activeFile.truncated ? t('dump.truncated') : ''}
                      {contentFilter.trim() ? t('dump.keywordFiltering') : ''}
                      {copyHint ? ` · ${copyHint}` : ''}
                    </span>
                    <label className="search-field logs-filter dump-content-filter">
                      <Icon name="search" size={14} />
                      <input
                        value={contentFilter}
                        onChange={(event) => onContentFilterChange(event.target.value)}
                        placeholder={t('dump.searchFileContent')}
                      />
                    </label>
                  </div>
                  <div className="logs-actions">
                    <button
                      className="text-button"
                      type="button"
                      onClick={() => downloadServerFile(activeFile.name)}
                    >
                      {t('dump.downloadOriginal')}
                    </button>
                  </div>
                </div>
                <div className="logs-scroll-shell dump-code-shell">
                  <pre
                    ref={dumpCodeRef}
                    className={`dump-code language-${activeFile.language}`}
                    dangerouslySetInnerHTML={{
                      __html: activeHtml || (contentFilter.trim() ? t('dump.noMatchContent') : ' '),
                    }}
                  />
                  <div className="logs-scroll-actions" aria-label={t('dump.scrollControl')}>
                    <button
                      className="button button-secondary logs-scroll-button"
                      type="button"
                      title={t('dump.toTop')}
                      onClick={() => {
                        requestAnimationFrame(() => scrollNode(dumpCodeRef.current, 'top'))
                      }}
                    >
                      <Icon name="toTop" size={14} />
                      {t('dump.toTop')}
                    </button>
                    <button
                      className="button button-secondary logs-scroll-button"
                      type="button"
                      title={t('dump.toBottom')}
                      onClick={() => {
                        requestAnimationFrame(() => scrollNode(dumpCodeRef.current, 'bottom'))
                      }}
                    >
                      <Icon name="toBottom" size={14} />
                      {t('dump.toBottom')}
                    </button>
                  </div>
                </div>
              </div>
            ) : (
              <div className="dump-empty">{t('dump.noFiles')}</div>
            )}
          </>
        )}
      </div>
    </div>
  )
}
