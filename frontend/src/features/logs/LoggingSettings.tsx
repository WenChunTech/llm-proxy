import { Icon } from '../../components/Icon'
import { SelectControl } from '../../components/controls/SelectControl'
import type { DebugDumpConfig } from '../../types/domain'
import { LOG_LEVEL_OPTIONS, type LogLevelPreset } from './format'

export function LoggingSettings({
  draftLogLevel,
  draftDebugDump,
  loggingDirty,
  isSaving,
  onLogLevelChange,
  onDebugDumpChange,
  onReset,
  onSave,
}: {
  draftLogLevel: LogLevelPreset
  draftDebugDump: DebugDumpConfig
  loggingDirty: boolean
  isSaving: boolean
  onLogLevelChange: (value: LogLevelPreset) => void
  onDebugDumpChange: (value: DebugDumpConfig | ((current: DebugDumpConfig) => DebugDumpConfig)) => void
  onReset: () => void
  onSave: () => void
}) {
  return (
    <section className="panel logging-settings-panel">
      <div className="panel-heading">
        <div>
          <h3>日志与转储配置</h3>
          <p className="panel-caption">
            保存后写入配置：日志等级即时生效，请求转储对后续请求生效
          </p>
        </div>
      </div>

      <div className="logging-settings-grid">
        <div className="logging-settings-card">
          <div className="logging-settings-card-title">
            <strong>日志等级</strong>
            <span className="logs-meta">log_level</span>
          </div>
          <p className="panel-description">控制进程日志详细程度，仅影响本服务相关输出。</p>
          <label className="field">
            <span>等级</span>
            <SelectControl
              ariaLabel="日志等级"
              mono
              value={draftLogLevel}
              options={[...LOG_LEVEL_OPTIONS]}
              onChange={onLogLevelChange}
            />
          </label>
        </div>

        <div className="logging-settings-card">
          <div className="logging-settings-card-title">
            <strong>请求转储</strong>
            <span className="logs-meta">debug_dump</span>
          </div>
          <p className="panel-description">
            开启后，每个代理请求会写入 request/response 原文，并显示「请求转储」页。
          </p>

          <div className="logging-settings-row between">
            <div>
              <strong>启用 debug_dump</strong>
              <p className="panel-description logging-settings-current">
                当前：{draftDebugDump.enabled ? '已开启' : '已关闭'}
              </p>
            </div>
            <button
              className={`toggle ${draftDebugDump.enabled ? 'on' : ''}`}
              type="button"
              aria-label="启用 debug_dump"
              onClick={() =>
                onDebugDumpChange((current) => ({
                  ...current,
                  enabled: !current.enabled,
                }))
              }
            >
              <span />
            </button>
          </div>

          <label className="field">
            <span>保存目录</span>
            <input
              value={draftDebugDump.dir}
              onChange={(event) =>
                onDebugDumpChange((current) => ({
                  ...current,
                  dir: event.target.value,
                }))
              }
              placeholder="logs"
              spellCheck={false}
              disabled={!draftDebugDump.enabled}
            />
            <small>相对进程工作目录，或使用绝对路径</small>
          </label>
        </div>
      </div>

      <div className="logging-settings-actions">
        <div className="logs-meta">
          {loggingDirty ? '有未保存的更改' : '已与服务端配置同步'}
          {isSaving ? ' · 正在保存…' : ''}
        </div>
        <div className="logs-actions">
          <button
            className="button button-secondary"
            type="button"
            disabled={!loggingDirty || isSaving}
            onClick={onReset}
          >
            重置
          </button>
          <button
            className="button button-primary"
            type="button"
            disabled={!loggingDirty || isSaving}
            onClick={onSave}
          >
            <Icon name="check" size={15} />
            保存配置
          </button>
        </div>
      </div>
    </section>
  )
}
