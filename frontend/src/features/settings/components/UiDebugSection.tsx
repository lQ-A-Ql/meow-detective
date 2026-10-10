import { useTranslation } from 'react-i18next';
import { Checkbox } from '@/app/components/ui/checkbox';
import type { LocalSettings } from '@/lib/settings';

interface UiDebugSectionProps {
  devEventTrace: boolean;
  setSettings: React.Dispatch<React.SetStateAction<LocalSettings>>;
}

export function UiDebugSection({
  devEventTrace,
  setSettings,
}: UiDebugSectionProps) {
  const { t } = useTranslation();

  return (
    <section>
      <div className="flex items-center gap-2 mb-3">
        <span className="text-[13px] font-light text-forensics-text-secondary">{t('settings.sections.uiDebug.title')}</span>
      </div>
      <div className="flex flex-wrap items-center gap-3 text-[12px]">
        <label className="flex items-center gap-2 border border-forensics-border bg-forensics-input-bg px-3 py-2">
          <Checkbox
            variant="forensics"
            checked={devEventTrace}
            onCheckedChange={(checked) =>
              setSettings((current) => ({
                ...current,
                devEventTrace: checked === true,
              }))
            }
          />
          {t('settings.sections.uiDebug.devEventTrace')}
        </label>
      </div>
    </section>
  );
}
