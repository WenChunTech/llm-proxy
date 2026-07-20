import { Icon } from './Icon'
import { defaultPriority, providerMeta } from '../config/providers'
import type { Provider, ProviderKind, ProviderKindFilter } from '../types/domain'

export function NavItem({
  icon,
  label,
  count,
  active,
  expanded,
  onClick,
}: {
  icon: string
  label: string
  count?: number
  active: boolean
  expanded?: boolean
  onClick: () => void
}) {
  return (
    <button className={`nav-item ${active ? 'active' : ''}`} type="button" onClick={onClick}>
      <Icon name={icon} size={18} />
      <span>{label}</span>
      {count !== undefined && <small>{count}</small>}
      {expanded !== undefined && (
        <span className={`nav-chevron ${expanded ? 'expanded' : ''}`}>
          <Icon name="chevron" size={13} />
        </span>
      )}
    </button>
  )
}

export function ProviderNavGroups({
  providers,
  activeKind,
  collapsed,
  onSelect,
}: {
  providers: Provider[]
  activeKind: ProviderKindFilter
  collapsed: boolean
  onSelect: (kind: ProviderKind) => void
}) {
  const counts = defaultPriority.map((kind) => ({
    kind,
    total: providers.filter((provider) => provider.kind === kind).length,
    enabled: providers.filter((provider) => provider.kind === kind && provider.enabled).length,
  }))

  return (
    <div className={`provider-nav-section ${collapsed ? 'is-collapsed' : ''}`}>
      {!collapsed && (
        <div className="provider-nav-groups" aria-label="提供商分组">
          {counts.map(({ kind, total, enabled }) => {
            const meta = providerMeta[kind]
            return (
              <button
                className={`provider-nav-item ${activeKind === kind ? 'active' : ''}`}
                type="button"
                key={kind}
                onClick={() => onSelect(kind)}
              >
                <span className={`provider-nav-mark ${kind === 'grok' ? 'grok-mark' : ''}`} style={{ backgroundColor: meta.color }} />
                <span>{meta.label}</span>
                <small>{enabled}/{total}</small>
              </button>
            )
          })}
        </div>
      )}
    </div>
  )
}
