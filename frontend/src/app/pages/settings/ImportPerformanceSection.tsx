import { useTranslation } from 'react-i18next';
import type { LocalSettings } from '@/lib/settings';

interface ImportPerformanceSectionProps {
  maxImportWorkers: string;
  maxAnalysisWorkers: string;
  importAnalysisMode: LocalSettings['importAnalysisMode'];
  setSettings: React.Dispatch<React.SetStateAction<LocalSettings>>;
}

export function ImportPerformanceSection({
  maxImportWorkers,
  maxAnalysisWorkers,
  importAnalysisMode,
  setSettings,
}: ImportPerformanceSectionProps) {
  const { t } = useTranslation();

  return (
    <section>
      <div className="flex items-center gap-2 mb-3">
        <span className="text-[13px] font-semibold text-forensics-text-secondary">{t('settings.sections.importPerformance.title')}</span>
      </div>
      <div className="grid gap-3 md:grid-cols-2">
        <label className="block border border-forensics-border bg-forensics-input-bg p-3">
          <span className="block text-[11px] font-semibold text-forensics-text-tertiary">{t('settings.sections.importPerformance.maxImportWorkers')}</span>
          <input
            value={maxImportWorkers}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxImportWorkers: event.target.value,
              }))
            }
            inputMode="numeric"
            placeholder={t('settings.sections.importPerformance.maxImportWorkers')}
            className="mt-2 w-full border border-forensics-border-strong bg-forensics-surface px-2 py-1 font-mono text-[12px]"
          />
          <span className="mt-1 block text-[10px] text-forensics-muted-lighter">{t('settings.sections.importPerformance.maxImportWorkersHint')}</span>
        </label>
        <label className="block border border-forensics-border bg-forensics-input-bg p-3">
          <span className="block text-[11px] font-semibold text-forensics-text-tertiary">{t('settings.sections.importPerformance.maxAnalysisWorkers')}</span>
          <input
            value={maxAnalysisWorkers}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxAnalysisWorkers: event.target.value,
              }))
            }
            inputMode="numeric"
            placeholder={t('settings.sections.importPerformance.maxAnalysisWorkers')}
            className="mt-2 w-full border border-forensics-border-strong bg-forensics-surface px-2 py-1 font-mono text-[12px]"
          />
          <span className="mt-1 block text-[10px] text-forensics-muted-lighter">{t('settings.sections.importPerformance.maxAnalysisWorkersHint')}</span>
        </label>
      </div>
      <label className="mt-3 block border border-forensics-border bg-forensics-input-bg p-3">
        <span className="block text-[11px] font-semibold text-forensics-text-tertiary">{t('settings.sections.importPerformance.importAnalysisMode')}</span>
        <select
          value={importAnalysisMode}
          onChange={(event) =>
            setSettings((current) => ({
              ...current,
              importAnalysisMode:
                event.target.value === 'fullContent'
                  ? 'fullContent'
                  : event.target.value === 'budgetedContent'
                    ? 'budgetedContent'
                    : 'metadataOnly',
            }))
          }
          className="mt-2 w-full border border-forensics-border-strong bg-forensics-surface px-2 py-1 text-[12px]"
        >
          <option value="metadataOnly">{t('settings.sections.importPerformance.metadataOnly')}</option>
          <option value="budgetedContent">{t('settings.sections.importPerformance.budgetedContent')}</option>
          <option value="fullContent">{t('settings.sections.importPerformance.fullContent')}</option>
        </select>
        <span className="mt-1 block text-[10px] text-forensics-muted-lighter">
          {t('settings.sections.importPerformance.importAnalysisModeHint')}
        </span>
      </label>
    </section>
  );
}
