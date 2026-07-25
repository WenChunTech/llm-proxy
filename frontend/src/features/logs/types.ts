export type PageTab = 'dumps' | 'process' | 'settings'

export type ConnectionState = 'connecting' | 'open' | 'closed' | 'error'

export type DumpSummary = {
  id: string
  model: string
  endpoint: string
  provider: string
  is_streaming: boolean
  status: number | null
  files: string[]
  mtime_ms: number
  matches?: string[]
}

export type DumpFile = {
  name: string
  size: number
  truncated: boolean
  content: string
  language: string
}

export type DumpDetail = {
  id: string
  model: string
  endpoint: string
  provider: string
  is_streaming: boolean
  status: number | null
  files: DumpFile[]
}

export type DumpListPayload = {
  enabled: boolean
  dir: string
  query?: string
  items: DumpSummary[]
}

export type DumpDeletePayload = {
  deleted: string[]
  failed: Array<{ id?: string; error?: string }>
}

export type DumpSocketEvent =
  | {
      type: 'hello'
      debug_dump?: { enabled?: boolean; dir?: string }
    }
  | {
      type: 'log'
      line: string
    }
  | {
      type: 'created' | 'updated'
      id: string
      model: string
      endpoint: string
      provider: string
      is_streaming: boolean
      status: number | null
      files: string[]
    }
  | {
      type: 'deleted'
      id: string
    }
  | {
      type: 'chunk'
      id: string
      file?: string
      text: string
    }
