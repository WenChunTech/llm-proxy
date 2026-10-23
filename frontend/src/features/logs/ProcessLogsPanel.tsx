import { useEffect, useMemo, useRef } from 'react'
import { useI18n } from '../../lib/i18n'
import { ansiToHtml, highlightKeywordInHtml } from './highlight'
import type { ProcessLogLine } from './types'
import { scrollNode } from './format'

export function ProcessLogsPanel({
  lines,
  filter,
  autoScroll,
}: {
  lines: ProcessLogLine[]
  filter: string
  autoScroll: boolean
}) {
  const { t } = useI18n()
  const processRef = useRef<HTMLPreElement | null>(null)
  const visibleLines = useMemo(() => {
    const query = filter.trim().toLowerCase()
    if (!query) return lines
    return lines.filter((line) => line.text.toLowerCase().includes(query))
  }, [filter, lines])

  useEffect(() => {
    if (!autoScroll) return
    scrollNode(processRef.current, 'bottom')
  }, [visibleLines, autoScroll])

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
                      __html: highlightKeywordInHtml(ansiToHtml(line.text), filter),
                    }}
                  />
                ) : (
                  <span dangerouslySetInnerHTML={{ __html: ansiToHtml(line.text) }} />
                )}
              </div>
            ))
          ) : (
            <div className="logs-empty">
              {filter.trim() ? t('process.noMatch') : t('process.noLogs')}
            </div>
          )}
        </pre>
      </div>
    </div>
  )
}
