import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef } from 'react'
import { Icon } from '../../components/Icon'
import { escapeHtml, highlightKeywordInHtml } from './highlight'
import type { ProcessLogLine } from './types'
import { scrollNode } from './format'

export type ProcessLogsPanelHandle = {
  scrollTo: (position: 'top' | 'bottom') => void
}

export const ProcessLogsPanel = forwardRef<
  ProcessLogsPanelHandle,
  {
    lines: ProcessLogLine[]
    filter: string
    autoScroll: boolean
    onAutoScrollChange?: (value: boolean) => void
  }
>(function ProcessLogsPanel({ lines, filter, autoScroll, onAutoScrollChange }, ref) {
  const processRef = useRef<HTMLPreElement | null>(null)
  const visibleLines = useMemo(() => {
    const query = filter.trim().toLowerCase()
    if (!query) return lines
    return lines.filter((line) => line.text.toLowerCase().includes(query))
  }, [filter, lines])

  useImperativeHandle(ref, () => ({
    scrollTo(position) {
      scrollNode(processRef.current, position)
    },
  }))

  useEffect(() => {
    if (!autoScroll) return
    scrollNode(processRef.current, 'bottom')
  }, [visibleLines, autoScroll])

  function scrollTo(position: 'top' | 'bottom') {
    if (position === 'top') onAutoScrollChange?.(false)
    else onAutoScrollChange?.(true)
    // Wait a frame so layout is settled when content is large.
    requestAnimationFrame(() => scrollNode(processRef.current, position))
  }

  return (
    <div className="logs-panel panel">
      <div className="logs-scroll-shell">
        <pre ref={processRef} className="logs-console" aria-live="polite">
          {visibleLines.length ? (
            visibleLines.map((line) => (
              <div className="logs-line" key={line.id}>
                {filter.trim() ? (
                  <span
                    dangerouslySetInnerHTML={{
                      __html: highlightKeywordInHtml(escapeHtml(line.text), filter),
                    }}
                  />
                ) : (
                  line.text
                )}
              </div>
            ))
          ) : (
            <div className="logs-empty">
              {filter.trim() ? '没有匹配的进程日志' : '暂无进程日志输出'}
            </div>
          )}
        </pre>
        <div className="logs-scroll-actions" aria-label="滚动控制">
          <button
            className="button button-secondary logs-scroll-button"
            type="button"
            title="置顶"
            onClick={() => scrollTo('top')}
          >
            <Icon name="toTop" size={14} />
            置顶
          </button>
          <button
            className="button button-secondary logs-scroll-button"
            type="button"
            title="置底"
            onClick={() => scrollTo('bottom')}
          >
            <Icon name="toBottom" size={14} />
            置底
          </button>
        </div>
      </div>
    </div>
  )
})
