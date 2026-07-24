import { forwardRef, useEffect, useImperativeHandle, useMemo, useRef } from 'react'
import { escapeHtml, highlightKeywordInHtml } from './highlight'
import { scrollNode } from './format'

export type ProcessLogsPanelHandle = {
  scrollTo: (position: 'top' | 'bottom') => void
}

export const ProcessLogsPanel = forwardRef<
  ProcessLogsPanelHandle,
  {
    lines: string[]
    filter: string
    autoScroll: boolean
  }
>(function ProcessLogsPanel({ lines, filter, autoScroll }, ref) {
  const processRef = useRef<HTMLPreElement | null>(null)
  const visibleLines = useMemo(() => {
    const query = filter.trim().toLowerCase()
    if (!query) return lines
    return lines.filter((line) => line.toLowerCase().includes(query))
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

  return (
    <div className="logs-panel panel">
      <pre ref={processRef} className="logs-console" aria-live="polite">
        {visibleLines.length ? (
          visibleLines.map((line, index) => (
            <div className="logs-line" key={`${index}-${line.slice(0, 32)}`}>
              {filter.trim() ? (
                <span
                  dangerouslySetInnerHTML={{
                    __html: highlightKeywordInHtml(escapeHtml(line), filter),
                  }}
                />
              ) : (
                line
              )}
            </div>
          ))
        ) : (
          <div className="logs-empty">
            {filter.trim() ? '没有匹配的进程日志' : '暂无进程日志输出'}
          </div>
        )}
      </pre>
    </div>
  )
})
