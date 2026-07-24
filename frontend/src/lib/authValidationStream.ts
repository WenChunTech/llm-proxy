import type {
  AuthProviderKind,
  AuthValidationPayload,
  AuthValidationResult,
  AuthValidationTarget,
} from '../types/domain'
import { buildAuthValidationConfig } from './authValidation'
import { buildDashboardWsUrl } from './ws'

export type AuthValidateStreamStarted = {
  type: 'started'
  kind: string
  model: string
  total: number
}

export type AuthValidateStreamResult = {
  type: 'result'
  completed: number
  total: number
  result: AuthValidationResult
}

export type AuthValidateStreamDone = {
  type: 'done'
  success: boolean
  data: AuthValidationPayload
}

export type AuthValidateStreamError = {
  type: 'error'
  message: string
}

export type AuthValidateStreamEvent =
  | AuthValidateStreamStarted
  | AuthValidateStreamResult
  | AuthValidateStreamDone
  | AuthValidateStreamError

export type AuthValidateStreamHandlers = {
  onStarted?: (event: AuthValidateStreamStarted) => void
  onResult?: (event: AuthValidateStreamResult) => void
  onDone?: (event: AuthValidateStreamDone) => void
  onError?: (message: string) => void
}

function parseStreamEvent(raw: string): AuthValidateStreamEvent | null {
  try {
    const value = JSON.parse(raw) as Record<string, unknown>
    const type = String(value.type ?? '')
    if (type === 'started') {
      return {
        type: 'started',
        kind: String(value.kind ?? ''),
        model: String(value.model ?? ''),
        total: Number(value.total ?? 0),
      }
    }
    if (type === 'result') {
      return {
        type: 'result',
        completed: Number(value.completed ?? 0),
        total: Number(value.total ?? 0),
        result: normalizeResult(value.result),
      }
    }
    if (type === 'done') {
      return {
        type: 'done',
        success: Boolean(value.success),
        data: normalizePayload(value.data),
      }
    }
    if (type === 'error') {
      return {
        type: 'error',
        message: String(value.message ?? 'validation failed'),
      }
    }
    return null
  } catch {
    return null
  }
}

function normalizeResult(value: unknown): AuthValidationResult {
  const record = (value && typeof value === 'object' ? value : {}) as Record<string, unknown>
  return {
    providerIndex: Number(record.providerIndex ?? record.provider_index ?? 0),
    authIndex: Number(record.authIndex ?? record.auth_index ?? 0),
    authCount: Number(record.authCount ?? record.auth_count ?? 0),
    isAuthArray: Boolean(record.isAuthArray ?? record.is_auth_array),
    label: String(record.label ?? ''),
    disabled: Boolean(record.disabled),
    skipped: Boolean(record.skipped),
    valid: Boolean(record.valid),
    reason: String(record.reason ?? ''),
    statusCode: Number(record.statusCode ?? record.status_code ?? 0),
    errorMessage: String(record.errorMessage ?? record.error_message ?? ''),
    refreshed: Boolean(record.refreshed),
    auth: record.auth,
  }
}

function normalizePayload(value: unknown): AuthValidationPayload {
  const record = (value && typeof value === 'object' ? value : {}) as Record<string, unknown>
  const results = Array.isArray(record.results)
    ? record.results.map((item) => normalizeResult(item))
    : []
  return {
    model: String(record.model ?? ''),
    total: Number(record.total ?? results.length),
    checked: Number(record.checked ?? 0),
    valid: Number(record.valid ?? 0),
    invalid: Number(record.invalid ?? 0),
    skipped: Number(record.skipped ?? 0),
    rateLimited: Number(record.rateLimited ?? record.rate_limited ?? 0),
    refreshed: Number(record.refreshed ?? 0),
    results,
  }
}

/**
 * Open a validation WebSocket, send the request payload, and stream progress events.
 * Returns an abort function that closes the socket.
 */
export function streamAuthValidation(options: {
  kind: AuthProviderKind
  accessKey: string
  providers: Parameters<typeof buildAuthValidationConfig>[0]
  targets?: AuthValidationTarget[]
  handlers: AuthValidateStreamHandlers
}): () => void {
  const { kind, accessKey, providers, targets, handlers } = options
  let settled = false
  const ws = new WebSocket(buildDashboardWsUrl(`/api/${kind}/validate/ws`, accessKey))

  function finishError(message: string) {
    if (settled) return
    settled = true
    handlers.onError?.(message)
  }

  ws.onopen = () => {
    const body = {
      config: buildAuthValidationConfig(providers),
      ...(targets?.length ? { targets } : {}),
    }
    ws.send(JSON.stringify(body))
  }

  ws.onmessage = (event) => {
    const raw = typeof event.data === 'string' ? event.data : String(event.data)
    const parsed = parseStreamEvent(raw)
    if (!parsed) return
    if (parsed.type === 'started') handlers.onStarted?.(parsed)
    if (parsed.type === 'result') handlers.onResult?.(parsed)
    if (parsed.type === 'done') {
      settled = true
      handlers.onDone?.(parsed)
    }
    if (parsed.type === 'error') finishError(parsed.message)
  }

  ws.onerror = () => {
    finishError('WebSocket 连接失败')
  }

  ws.onclose = () => {
    if (!settled) finishError('校验连接已断开')
  }

  return () => {
    settled = true
    if (ws.readyState === WebSocket.OPEN || ws.readyState === WebSocket.CONNECTING) {
      ws.close()
    }
  }
}
