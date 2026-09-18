import { Icon } from '../../components/Icon'
import { useI18n } from '../../lib/i18n'
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
  const { t } = useI18n()
  const target = { providerIndex: result.providerIndex, authIndex: result.authIndex }
  const statusClass = validating
    ? 'pending'
    : result.reason === 'rate_limited'
      ? 'skipped'
      : result.valid ? 'ok' : result.skipped ? 'skipped' : 'error'

  async function copyAuthCurl() {
    const curl = result.curl.trim()
    if (!curl) {
      setToast(t('authVal.noCurl'))
      return
    }
    try {
      await copyText(curl)
      setToast(t('authVal.curlCopied'))
    } catch {
      setToast(t('authVal.copyFailed'))
    }
  }

  return (
    <div className={`auth-validation-result-row ${validating ? 'is-validating' : ''}`}>
      <div className="auth-validation-result-main">
        <span className={`status-dot ${statusClass}`} />
        <div>
          <strong>{result.label}</strong>
          <span>
            {t('authVal.configNumber', { index: result.providerIndex + 1 })}
            {result.authCount > 1 ? t('authVal.authNumber', { index: result.authIndex + 1 }) : ''}
          </span>
        </div>
      </div>
      <div className="auth-validation-result-detail">
        <code>{authValidationReasonLabel(result.reason)}</code>
        {result.statusCode > 0 && <span>HTTP {result.statusCode}</span>}
        {result.refreshed && <span>{t('authVal.refreshed')}</span>}
        {result.reason === 'rate_limited' && result.disabled && <span>{t('authVal.autoDisabled')}</span>}
        {result.disabled && <span>{t('authVal.disabled')}</span>}
        {validating && <span className="auth-validation-running">{t('authVal.validating')}</span>}
        {result.errorMessage && !validating && (
          <pre className="auth-validation-error" title={t('authVal.upstreamRaw')}>
            {result.errorMessage}
          </pre>
        )}
      </div>
      <div className="auth-validation-row-actions">
        <button
          className="icon-button"
          type="button"
          title={validating ? t('authVal.validatingShort') : t('authVal.revalidate')}
          aria-label={validating ? t('authVal.validatingShort') : t('authVal.revalidate')}
          disabled={disabled || validating}
          onClick={() => onValidate(kind, target)}
        >
          <Icon name={validating ? 'pulse' : 'refresh'} size={16} />
        </button>
        <button
          className="icon-button"
          type="button"
          title={result.disabled ? t('authVal.enableAuth') : t('authVal.disableAuth')}
          aria-label={result.disabled ? t('authVal.enableAuth') : t('authVal.disableAuth')}
          disabled={validating}
          onClick={() => onDisable(kind, target, !result.disabled)}
        >
          <Icon name={result.disabled ? 'check' : 'ban'} size={16} />
        </button>
        <button
          className="icon-button"
          type="button"
          title={t('authVal.copyCurl')}
          aria-label={t('authVal.copyCurl')}
          disabled={!result.curl}
          onClick={copyAuthCurl}
        >
          <Icon name="copy" size={16} />
        </button>
        <button
          className="icon-button danger-button"
          type="button"
          title={t('authVal.deleteAuth')}
          aria-label={t('authVal.deleteAuth')}
          disabled={validating}
          onClick={() => onDelete(kind, target)}
        >
          <Icon name="trash" size={15} />
        </button>
      </div>
    </div>
  )
}
