import { Icon } from '../../components/Icon'
import { authValidationReasonLabel } from '../../lib/authValidation'
import { copyText } from '../../lib/browser'
import type {
  AuthProviderKind,
  AuthValidationState,
  AuthValidationTarget,
} from '../../types/domain'

export function AuthValidationResultRow({
  kind,
  result,
  disabled,
  validating = false,
  onValidate,
  onDisable,
  onDelete,
  setToast,
}: {
  kind: AuthProviderKind
  result: AuthValidationState['payload']['results'][number]
  disabled: boolean
  validating?: boolean
  onValidate: (kind: AuthProviderKind, target: AuthValidationTarget) => void
  onDisable: (kind: AuthProviderKind, target: AuthValidationTarget, disabled: boolean) => void
  onDelete: (kind: AuthProviderKind, target: AuthValidationTarget) => void
  setToast: (message: string) => void
}) {
  const target = { providerIndex: result.providerIndex, authIndex: result.authIndex }
  const statusClass = validating
    ? 'pending'
    : result.reason === 'rate_limited'
      ? 'skipped'
      : result.valid ? 'ok' : result.skipped ? 'skipped' : 'error'

  async function copyAuthCurl() {
    const curl = result.curl.trim()
    if (!curl) {
      setToast('当前结果没有可复制的 curl 命令')
      return
    }
    try {
      await copyText(curl)
      setToast('curl 命令已复制')
    } catch {
      setToast('复制失败')
    }
  }

  return (
    <div className={`auth-validation-result-row ${validating ? 'is-validating' : ''}`}>
      <div className="auth-validation-result-main">
        <span className={`status-dot ${statusClass}`} />
        <div>
          <strong>{result.label}</strong>
          <span>
            配置 #{result.providerIndex + 1}
            {result.authCount > 1 ? ` / Auth #${result.authIndex + 1}` : ''}
          </span>
        </div>
      </div>
      <div className="auth-validation-result-detail">
        <code>{authValidationReasonLabel(result.reason)}</code>
        {result.statusCode > 0 && <span>HTTP {result.statusCode}</span>}
        {result.refreshed && <span>已刷新</span>}
        {result.reason === 'rate_limited' && result.disabled && <span>已自动禁用</span>}
        {result.disabled && <span>已禁用</span>}
        {validating && <span className="auth-validation-running">校验中…</span>}
        {result.errorMessage && !validating && <span className="auth-validation-error">{result.errorMessage}</span>}
      </div>
      <div className="auth-validation-row-actions">
        <button
          className="icon-button"
          type="button"
          title={validating ? '校验中' : '重新校验这条 Auth 配置'}
          aria-label={validating ? '校验中' : '重新校验这条 Auth 配置'}
          disabled={disabled || validating}
          onClick={() => onValidate(kind, target)}
        >
          <Icon name={validating ? 'pulse' : 'refresh'} size={16} />
        </button>
        <button
          className="icon-button"
          type="button"
          title={result.disabled ? '启用 Auth' : '禁用 Auth'}
          aria-label={result.disabled ? '启用 Auth' : '禁用 Auth'}
          disabled={validating}
          onClick={() => onDisable(kind, target, !result.disabled)}
        >
          <Icon name={result.disabled ? 'check' : 'ban'} size={16} />
        </button>
        <button
          className="icon-button"
          type="button"
          title="复制触发当前校验响应的 curl 命令"
          aria-label="复制触发当前校验响应的 curl 命令"
          disabled={!result.curl}
          onClick={copyAuthCurl}
        >
          <Icon name="copy" size={16} />
        </button>
        <button
          className="icon-button danger-button"
          type="button"
          title="删除 auth"
          aria-label="删除 auth"
          disabled={validating}
          onClick={() => onDelete(kind, target)}
        >
          <Icon name="trash" size={15} />
        </button>
      </div>
    </div>
  )
}
