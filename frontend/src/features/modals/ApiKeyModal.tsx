import { useEffect, useState } from 'react'
import { Icon } from '../../components/Icon'
import { useI18n } from '../../lib/i18n'

export function ApiKeyModal({
  apiKey,
  onClose,
  onSubmit,
}: {
  apiKey: string
  onClose: () => void
  onSubmit: (apiKey: string) => void
}) {
  const { t } = useI18n()
  const [value, setValue] = useState(apiKey)

  useEffect(() => {
    setValue(apiKey)
  }, [apiKey])

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    onSubmit(value.trim())
  }

  return (
    <div className="llm-modal-backdrop" onMouseDown={onClose}>
      <form className="modal auth-modal" onSubmit={submit} onMouseDown={(event) => event.stopPropagation()}>
        <div className="modal-heading">
          <div>
            <span className="eyebrow">{t('auth.dashboardAccess')}</span>
            <h2>{t('apiKey.title')}</h2>
          </div>
          <button className="icon-button" type="button" title={t('apiKey.close')} onClick={onClose}>
            ×
          </button>
        </div>
        <label className="field">
          <span>API Key</span>
          <input
            autoFocus
            type="password"
            value={value}
            onChange={(event) => setValue(event.target.value)}
            placeholder={t('apiKey.placeholder')}
          />
        </label>
        <div className="modal-actions">
          <button className="button button-secondary" type="button" onClick={onClose}>
            {t('apiKey.cancel')}
          </button>
          <button className="button button-primary" type="submit">
            <Icon name="check" size={16} />
            {t('apiKey.save')}
          </button>
        </div>
      </form>
    </div>
  )
}
