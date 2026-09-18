import { Icon } from './Icon'
import { useI18n } from '../lib/i18n'

export function LanguageSwitcher() {
  const { language, setLanguage, t } = useI18n()

  return (
    <button
      className="language-switcher"
      type="button"
      title={t('lang.switch')}
      aria-label={t('lang.switch')}
      onClick={() => setLanguage(language === 'en' ? 'zh' : 'en')}
    >
      <Icon name="terminal" size={15} />
      <span>{language === 'en' ? '中' : 'EN'}</span>
    </button>
  )
}
