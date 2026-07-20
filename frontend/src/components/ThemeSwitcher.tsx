import { Icon } from './Icon'
import { SelectControl } from './controls/SelectControl'
import type { ThemeMode } from '../types/domain'
import type { SelectOption } from './controls/SelectControl'

export function ThemeSwitcher({
  value,
  onChange,
}: {
  value: ThemeMode
  onChange: (value: ThemeMode) => void
}) {
  const meta: Record<ThemeMode, { icon: string; label: string }> = {
    light: { icon: 'sun', label: 'Light' },
    dark: { icon: 'moon', label: 'Dark' },
    system: { icon: 'monitor', label: 'System' },
  }

  const options: SelectOption<ThemeMode>[] = [
    { value: 'system', label: 'System' },
    { value: 'light', label: 'Light' },
    { value: 'dark', label: 'Dark' },
  ]

  return (
    <div className="theme-switcher" title="切换主题">
      <Icon name={meta[value].icon} size={15} />
      <SelectControl
        compact
        value={value}
        options={options}
        onChange={onChange}
        ariaLabel="切换主题"
      />
    </div>
  )
}
