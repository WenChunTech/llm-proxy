import { Icon } from '../Icon'

export function LoginOverlay({
  value,
  onChange,
  onSubmit,
}: {
  value: string
  onChange: (value: string) => void
  onSubmit: (event: React.FormEvent<HTMLFormElement>) => void
}) {
  return (
    <div className="login-overlay">
      <form className="login-panel" onSubmit={onSubmit}>
        <span className="eyebrow">DASHBOARD ACCESS</span>
        <h2>输入 API Key</h2>
        <label className="field">
          <span>API Key</span>
          <input
            autoFocus
            type="password"
            value={value}
            onChange={(event) => onChange(event.target.value)}
            placeholder="用于访问管理页面"
          />
        </label>
        <button className="button button-primary" type="submit">
          <Icon name="check" size={16} />
          登录
        </button>
      </form>
    </div>
  )
}

export function AccessCheckingOverlay() {
  return (
    <div className="login-overlay">
      <div className="login-panel">
        <span className="eyebrow">DASHBOARD ACCESS</span>
        <h2>正在校验访问权限</h2>
        <span className="muted-copy">请稍候</span>
      </div>
    </div>
  )
}
