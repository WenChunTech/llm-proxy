import { Icon } from './Icon'
import { SelectControl } from './controls/SelectControl'
import { useI18n } from '../lib/i18n'
import type { ThemeMode } from '../types/domain'
import type { SelectOption } from './controls/SelectControl'

export function ThemeSwitcher({
  value,
  onChange,
}: {
  value: ThemeMode
  onChange: (value: ThemeMode) => void
}) {
  const { t } = useI18n()
  const meta: Record<ThemeMode, { icon: string; label: string }> = {
    light: { icon: 'sun', label: t('theme.light') },
    dark: { icon: 'moon', label: t('theme.dark') },
    system: { icon: 'monitor', label: t('theme.system') },
  }

  const options: SelectOption<ThemeMode>[] = [
    { value: 'system', label: t('theme.system') },
    { value: 'light', label: t('theme.light') },
    { value: 'dark', label: t('theme.dark') },
  ]

  return (
    <div className="theme-switcher" title={t('theme.switch')}>
      <Icon name={meta[value].icon} size={15} />
      <SelectControl
        compact
        value={value}
        options={options}
        onChange={onChange}
        ariaLabel={t('theme.switch')}
      />
    </div>
  )
}
