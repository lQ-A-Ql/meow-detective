import { useTranslation } from 'react-i18next';
import type { LocalSettings } from '@/lib/settings';

interface UiDebugSectionProps {
  theme: LocalSettings['theme'];
  devEventTrace: boolean;
  savingSettings: boolean;
  settingsMessage: string;
  setSettings: React.Dispatch<React.SetStateAction<LocalSettings>>;
  onSave: () => void;
}

export function UiDebugSection({
  theme,
  devEventTrace,
  savingSettings,
  settingsMessage,
  setSettings,
  onSave,
}: UiDebugSectionProps) {
  const { t } = useTranslation();

  return (
    <section>
      <div className="flex items-center gap-2 mb-3">
        <span className="text-[13px] font-semibold text-forensics-text-secondary">{t('settings.sections.uiDebug.title')}</span>
      </div>
      <div className="flex flex-wrap items-center gap-3 text-[12px]">
        <label
          htmlFor="settings-theme"
          className="flex items-center gap-2 border border-forensics-border bg-forensics-input-bg px-3 py-2"
        >
          {t('settings.sections.uiDebug.theme')}
          <select
            id="settings-theme"
            value={theme}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                theme: event.target.value === 'dark' ? 'dark' : 'light',
              }))
            }
            className="border border-forensics-border-strong bg-forensics-surface px-2 py-1"
          >
            <option value="light">{t('settings.sections.uiDebug.themeLight')}</option>
            <option value="dark">{t('settings.sections.uiDebug.themeDark')}</option>
          </select>
        </label>
        <label className="flex items-center gap-2 border border-forensics-border bg-forensics-input-bg px-3 py-2">
          <input
            type="checkbox"
            checked={devEventTrace}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                devEventTrace: event.target.checked,
              }))
            }
          />
          {t('settings.sections.uiDebug.devEventTrace')}
        </label>
        <button
          type="button"
          onClick={onSave}
          disabled={savingSettings}
          className="border border-forensics-text bg-forensics-text px-4 py-2 text-[12px] text-forensics-surface hover:bg-forensics-text-secondary"
        >
          {savingSettings ? t('settings.saving') : t('settings.save')}
        </button>
        {settingsMessage ? (
          <span className="text-[11px] text-forensics-muted">{settingsMessage}</span>
        ) : null}
      </div>
    </section>
  );
}
