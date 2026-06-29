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
  return (
    <section>
      <div className="flex items-center gap-2 mb-3">
        <span className="text-[13px] font-semibold text-[#333]">界面与调试</span>
      </div>
      <div className="flex flex-wrap items-center gap-3 text-[12px]">
        <label
          htmlFor="settings-theme"
          className="flex items-center gap-2 border border-[#e0e0e0] bg-[#f8f8f8] px-3 py-2"
        >
          主题
          <select
            id="settings-theme"
            value={theme}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                theme: event.target.value === 'dark' ? 'dark' : 'light',
              }))
            }
            className="border border-[#ccc] bg-white px-2 py-1"
          >
            <option value="light">浅色</option>
            <option value="dark">深色</option>
          </select>
        </label>
        <label className="flex items-center gap-2 border border-[#e0e0e0] bg-[#f8f8f8] px-3 py-2">
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
          事件调试日志
        </label>
        <button
          type="button"
          onClick={onSave}
          disabled={savingSettings}
          className="border border-[#111] bg-[#111] px-4 py-2 text-[12px] text-white hover:bg-[#333]"
        >
          {savingSettings ? '保存中...' : '保存设置'}
        </button>
        {settingsMessage ? (
          <span className="text-[11px] text-[#666]">{settingsMessage}</span>
        ) : null}
      </div>
    </section>
  );
}
