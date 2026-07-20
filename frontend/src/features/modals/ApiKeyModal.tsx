import { useEffect, useState } from 'react'
import { Icon } from '../../components/Icon'

export function ApiKeyModal({
  apiKey,
  onClose,
  onSubmit,
}: {
  apiKey: string
  onClose: () => void
  onSubmit: (apiKey: string) => void
}) {
  const [value, setValue] = useState(apiKey)

  useEffect(() => {
    setValue(apiKey)
  }, [apiKey])

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault()
    onSubmit(value.trim())
  }

  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <form className="modal auth-modal" onSubmit={submit} onMouseDown={(event) => event.stopPropagation()}>
        <div className="modal-heading">
          <div>
            <span className="eyebrow">DASHBOARD ACCESS</span>
            <h2>修改 API Key</h2>
          </div>
          <button className="icon-button" type="button" title="关闭" onClick={onClose}>
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
            placeholder="输入新的 API Key"
          />
        </label>
        <div className="modal-actions">
          <button className="button button-secondary" type="button" onClick={onClose}>
            取消
          </button>
          <button className="button button-primary" type="submit">
            <Icon name="check" size={16} />
            保存 API Key
          </button>
        </div>
      </form>
    </div>
  )
}
