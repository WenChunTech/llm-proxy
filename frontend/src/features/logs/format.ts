export const LOG_LEVEL_OPTIONS = [
  { value: 'error', label: 'error' },
  { value: 'warn', label: 'warn' },
  { value: 'info', label: 'info' },
  { value: 'debug', label: 'debug' },
  { value: 'trace', label: 'trace' },
] as const

export type LogLevelPreset = (typeof LOG_LEVEL_OPTIONS)[number]['value']

export function normalizeLogLevel(value: string): LogLevelPreset {
  return LOG_LEVEL_OPTIONS.some((option) => option.value === value)
    ? (value as LogLevelPreset)
    : 'info'
}

export function formatBytes(size: number) {
  if (size < 1024) return `${size} B`
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`
  return `${(size / (1024 * 1024)).toFixed(2)} MB`
}

export function formatDumpTime(id: string, mtimeMs: number) {
  const match = id.match(/^(\d{4})(\d{2})(\d{2})_(\d{2})(\d{2})(\d{2})_(\d{3})/)
  if (match) {
    const [, y, mo, d, h, mi, s, ms] = match
    return `${y}-${mo}-${d} ${h}:${mi}:${s}.${ms}`
  }
  if (mtimeMs > 0) return new Date(mtimeMs).toLocaleString()
  return id
}

export function statusClass(status: number | null | undefined) {
  if (status == null) return 'is-pending'
  if (status >= 200 && status < 300) return 'is-ok'
  if (status >= 400) return 'is-error'
  return 'is-warn'
}

export function preferFile(files: string[]): string {
  if (files.includes('request.json')) return 'request.json'
  if (files.includes('response.json')) return 'response.json'
  if (files.includes('response.sse')) return 'response.sse'
  if (files.includes('meta.json')) return 'meta.json'
  return files[0] ?? ''
}

export function filterLinesByKeyword(content: string, keyword: string) {
  const query = keyword.trim().toLowerCase()
  if (!query) return content
  return content
    .split(/\r?\n/)
    .filter((line) => line.toLowerCase().includes(query))
    .join('\n')
}

export function scrollNode(node: HTMLElement | null, position: 'top' | 'bottom') {
  if (!node) return
  node.scrollTop = position === 'top' ? 0 : node.scrollHeight
}

export function connectionLabel(connection: 'connecting' | 'open' | 'closed' | 'error') {
  if (connection === 'open') return '实时连接'
  if (connection === 'connecting') return '连接中…'
  if (connection === 'error') return '连接错误'
  return '已断开，重连中…'
}
