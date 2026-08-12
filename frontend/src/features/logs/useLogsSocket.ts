import { useEffect, useRef, useState } from 'react'
import { buildDashboardWsUrl } from '../../lib/ws'
import type { ConnectionState, DumpSummary, ProcessLogLine } from './types'

const MAX_PROCESS_LINES = 5_000
// Coalesce bursts of log lines into a single state update per flush so high
// log volume does not trigger N setStates (and N full-array copies) per frame.
const FLUSH_INTERVAL_MS = 80

type DumpLifecycleEvent = {
  type: 'created' | 'updated'
  summary: DumpSummary
}

type DumpDeletedEvent = {
  id: string
}

type DumpChunkEvent = {
  id: string
  text: string
}

type UseLogsSocketOptions = {
  accessKey: string
  dumpEnabled: boolean
  onDumpsDir?: (dir: string) => void
  onDumpEvent?: (event: DumpLifecycleEvent) => void
  onDumpDeleted?: (event: DumpDeletedEvent) => void
  onChunk?: (event: DumpChunkEvent) => void
}

export function useLogsSocket({
  accessKey,
  dumpEnabled,
  onDumpsDir,
  onDumpEvent,
  onDumpDeleted,
  onChunk,
}: UseLogsSocketOptions) {
  const [connection, setConnection] = useState<ConnectionState>('connecting')
  const [processLines, setProcessLines] = useState<ProcessLogLine[]>([])
  const onDumpsDirRef = useRef(onDumpsDir)
  const onDumpEventRef = useRef(onDumpEvent)
  const onDumpDeletedRef = useRef(onDumpDeleted)
  const onChunkRef = useRef(onChunk)
  onDumpsDirRef.current = onDumpsDir
  onDumpEventRef.current = onDumpEvent
  onDumpDeletedRef.current = onDumpDeleted
  onChunkRef.current = onChunk

  useEffect(() => {
    let disposed = false
    let reconnectTimer: number | null = null
    let ws: WebSocket | null = null
    // Pending lines awaiting the next flush; flushed on an interval to avoid a
    // setState per websocket message.
    const pending: ProcessLogLine[] = []
    let nextId = 1

    function appendProcessLine(text: string) {
      if (!text) return
      pending.push({ id: nextId++, text })
    }

    function flushPending() {
      if (pending.length === 0) return
      const batch = pending.splice(0, pending.length)
      setProcessLines((current) => {
        const merged =
          current.length + batch.length > MAX_PROCESS_LINES
            ? current.slice(Math.max(0, current.length + batch.length - MAX_PROCESS_LINES))
            : current.slice()
        for (const line of batch) merged.push(line)
        return merged
      })
    }

    const flushTimer = window.setInterval(flushPending, FLUSH_INTERVAL_MS)

    function connect() {
      if (disposed) return
      setConnection('connecting')
      ws = new WebSocket(buildDashboardWsUrl('/api/logs/ws', accessKey))
      ws.onopen = () => {
        if (!disposed) setConnection('open')
      }
      ws.onerror = () => {
        if (!disposed) setConnection('error')
      }
      ws.onclose = () => {
        if (disposed) return
        setConnection('closed')
        reconnectTimer = window.setTimeout(connect, 1500)
      }
      ws.onmessage = (event) => {
        if (disposed) return
        let payload: Record<string, unknown>
        try {
          payload = JSON.parse(typeof event.data === 'string' ? event.data : String(event.data))
        } catch {
          appendProcessLine(String(event.data))
          return
        }

        const type = String(payload.type ?? '')
        if (type === 'hello') {
          const dump = payload.debug_dump as { enabled?: boolean; dir?: string } | undefined
          if (dump?.dir) onDumpsDirRef.current?.(dump.dir)
          return
        }
        if (type === 'log') {
          appendProcessLine(String(payload.line ?? ''))
          return
        }
        if (!dumpEnabled) return
        if (type === 'created' || type === 'updated') {
          const summary: DumpSummary = {
            id: String(payload.id ?? ''),
            model: String(payload.model ?? ''),
            endpoint: String(payload.endpoint ?? ''),
            provider: String(payload.provider ?? ''),
            is_streaming: Boolean(payload.is_streaming),
            status: typeof payload.status === 'number' ? payload.status : null,
            files: Array.isArray(payload.files) ? (payload.files as string[]) : [],
            mtime_ms: Date.now(),
          }
          if (!summary.id) return
          onDumpEventRef.current?.({ type, summary })
          return
        }
        if (type === 'deleted') {
          const id = String(payload.id ?? '')
          if (!id) return
          onDumpDeletedRef.current?.({ id })
          return
        }
        if (type === 'chunk') {
          const id = String(payload.id ?? '')
          const text = String(payload.text ?? '')
          if (!id || !text) return
          onChunkRef.current?.({ id, text })
        }
      }
    }

    connect()
    return () => {
      disposed = true
      window.clearInterval(flushTimer)
      if (reconnectTimer !== null) window.clearTimeout(reconnectTimer)
      ws?.close()
    }
  }, [accessKey, dumpEnabled])

  return {
    connection,
    processLines,
    setProcessLines,
  }
}

export function appendLiveChunk(
  current: Record<string, string>,
  id: string,
  text: string,
): Record<string, string> {
  return {
    ...current,
    [id]: `${current[id] ?? ''}${text}`,
  }
}
