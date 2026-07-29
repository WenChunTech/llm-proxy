export type View = 'providers' | 'routing' | 'logs'
export type ThemeMode = 'light' | 'dark' | 'system'
export type AuthStatus = 'checking' | 'ready' | 'login'

export type ProviderKind =
  | 'openai_chat'
  | 'openai_responses'
  | 'claude'
  | 'gemini'
  | 'codex'
  | 'grok'

export type ProviderKindFilter = 'all' | ProviderKind

export type Provider = {
  id: string
  kind: ProviderKind
  name: string
  baseUrl: string
  apiKey: string
  models: string[]
  headers: Record<string, string>
  enabled: boolean
  auth?: unknown
}

export type ProviderDraft = Omit<Provider, 'id'>

export type RetryConfig = {
  maxRetries: number
  backoffStepMs: number
}

export type ApiProvider = {
  id: string
  kind: ProviderKind
  name: string
  enabled: boolean
  base_url: string
  api_key: string
  models: string[]
  headers: Record<string, string>
  auth?: unknown
}

export type DebugDumpConfig = {
  enabled: boolean
  dir: string
}

export type DashboardPayload = {
  port: number
  providers: ApiProvider[]
  model_priority: ProviderKind[]
  fallback_models: string[]
  model_aliases: Record<string, string[]>
  retry: {
    max_retries: number
    backoff_step_ms: number
  }
  api_key: string
  api_key_enabled: boolean
  log_level?: string | null
  debug_dump?: DebugDumpConfig
}

export type ApiModel = {
  id: string
  owned_by: string
  root?: string
}

export type ModelsPayload = {
  data: ApiModel[]
}

export type ProviderModelEntry = string | { id?: string }

export type ProviderModelsPayload = {
  endpoint?: string
  data: ProviderModelEntry[]
}

export type ProviderTestPayload = {
  ok: boolean
  status: number
  provider: string
  model: string
  stream: boolean
  raw_body: string
  body_preview: string
}

export type AuthValidationResult = {
  providerIndex: number
  authIndex: number
  authCount: number
  isAuthArray: boolean
  label: string
  disabled: boolean
  skipped: boolean
  valid: boolean
  reason: string
  statusCode: number
  errorMessage: string
  refreshed: boolean
  auth: unknown
}

export type AuthValidationPayload = {
  model: string
  total: number
  checked: number
  valid: number
  invalid: number
  skipped: number
  rateLimited: number
  refreshed: number
  results: AuthValidationResult[]
}

export type AuthProviderKind = 'codex' | 'grok'
export type AuthValidationFilter = 'all' | 'ok' | 'invalid' | 'rate_limited' | 'disabled'

export type AuthValidationTarget = {
  providerIndex: number
  authIndex: number
}

export type AuthValidationState = {
  kind: AuthProviderKind
  payload: AuthValidationPayload
  filter: AuthValidationFilter
}

export type AuthValidationResponse = {
  success: boolean
  data: AuthValidationPayload
}
