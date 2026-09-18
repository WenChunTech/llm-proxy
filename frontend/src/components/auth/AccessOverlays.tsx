import { Icon } from '../Icon'
import { useI18n } from '../../lib/i18n'

export function LoginOverlay({
  value,
  onChange,
  onSubmit,
}: {
  value: string
  onChange: (value: string) => void
  onSubmit: (event: React.FormEvent<HTMLFormElement>) => void
}) {
  const { t } = useI18n()
  return (
    <div className="login-overlay">
      <form className="login-panel" onSubmit={onSubmit}>
        <span className="eyebrow">{t('auth.dashboardAccess')}</span>
        <h2>{t('auth.enterApiKey')}</h2>
        <label className="field">
          <span>API Key</span>
          <input
            autoFocus
            type="password"
            value={value}
            onChange={(event) => onChange(event.target.value)}
            placeholder={t('auth.apiKeyPlaceholder')}
          />
        </label>
        <button className="button button-primary" type="submit">
          <Icon name="check" size={16} />
          {t('auth.login')}
        </button>
      </form>
    </div>
  )
}

export function AccessCheckingOverlay() {
  const { t } = useI18n()
  return (
    <div className="login-overlay">
      <div className="login-panel">
        <span className="eyebrow">{t('auth.dashboardAccess')}</span>
        <h2>{t('auth.checkingAccess')}</h2>
        <span className="muted-copy">{t('auth.pleaseWait')}</span>
      </div>
    </div>
  )
}
