import { Icon } from '../../components/Icon'
import { SelectControl } from '../../components/controls/SelectControl'
import { useI18n } from '../../lib/i18n'
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
  const { t } = useI18n()
  return (
    <section className="panel logging-settings-panel">
      <div className="panel-heading">
        <div>
          <h3>{t('logSettings.title')}</h3>
          <p className="panel-caption">
            {t('logSettings.caption')}
          </p>
        </div>
      </div>

      <div className="logging-settings-grid">
        <div className="logging-settings-card">
          <div className="logging-settings-card-title">
            <strong>{t('logSettings.logLevelTitle')}</strong>
            <span className="logs-meta">log_level</span>
          </div>
          <p className="panel-description">{t('logSettings.logLevelDesc')}</p>
          <label className="field">
            <span>{t('logSettings.levelLabel')}</span>
            <SelectControl
              ariaLabel={t('logSettings.logLevelTitle')}
              mono
              value={draftLogLevel}
              options={[...LOG_LEVEL_OPTIONS]}
              onChange={onLogLevelChange}
            />
          </label>
        </div>

        <div className="logging-settings-card">
          <div className="logging-settings-card-title">
            <strong>{t('logSettings.dumpTitle')}</strong>
            <span className="logs-meta">debug_dump</span>
          </div>
          <p className="panel-description">
            {t('logSettings.dumpDesc')}
          </p>

          <div className="logging-settings-row between">
            <div>
              <strong>{t('logSettings.enableDump')}</strong>
              <p className="panel-description logging-settings-current">
                {draftDebugDump.enabled ? t('logSettings.currentOn') : t('logSettings.currentOff')}
              </p>
            </div>
            <button
              className={`toggle ${draftDebugDump.enabled ? 'on' : ''}`}
              type="button"
              aria-label={t('logSettings.enableDump')}
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
            <span>{t('logSettings.saveDir')}</span>
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
            <small>{t('logSettings.dirHint')}</small>
          </label>
        </div>
      </div>

      <div className="logging-settings-actions">
        <div className="logs-meta">
          {loggingDirty ? t('logSettings.dirty') : t('logSettings.synced')}
          {isSaving ? t('logSettings.saving') : ''}
        </div>
        <div className="logs-actions">
          <button
            className="button button-secondary"
            type="button"
            disabled={!loggingDirty || isSaving}
            onClick={onReset}
          >
            {t('logSettings.reset')}
          </button>
          <button
            className="button button-primary"
            type="button"
            disabled={!loggingDirty || isSaving}
            onClick={onSave}
          >
            <Icon name="check" size={15} />
            {t('logSettings.saveConfig')}
          </button>
        </div>
      </div>
    </section>
  )
}
