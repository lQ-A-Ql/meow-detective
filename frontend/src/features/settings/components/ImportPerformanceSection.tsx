import { useTranslation } from 'react-i18next';
import { FieldHint } from '@/app/components/ui/field';
import { Input } from '@/app/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/app/components/ui/select';
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
          <Input
            value={maxImportWorkers}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxImportWorkers: event.target.value,
              }))
            }
            inputMode="numeric"
            placeholder={t('settings.sections.importPerformance.maxImportWorkers')}
            variant="numeric"
            inputSize="compact"
            className="mt-2"
          />
          <FieldHint className="mt-1 text-[10px] text-forensics-muted-lighter">{t('settings.sections.importPerformance.maxImportWorkersHint')}</FieldHint>
        </label>
        <label className="block border border-forensics-border bg-forensics-input-bg p-3">
          <span className="block text-[11px] font-semibold text-forensics-text-tertiary">{t('settings.sections.importPerformance.maxAnalysisWorkers')}</span>
          <Input
            value={maxAnalysisWorkers}
            onChange={(event) =>
              setSettings((current) => ({
                ...current,
                maxAnalysisWorkers: event.target.value,
              }))
            }
            inputMode="numeric"
            placeholder={t('settings.sections.importPerformance.maxAnalysisWorkers')}
            variant="numeric"
            inputSize="compact"
            className="mt-2"
          />
          <FieldHint className="mt-1 text-[10px] text-forensics-muted-lighter">{t('settings.sections.importPerformance.maxAnalysisWorkersHint')}</FieldHint>
        </label>
      </div>
      <label className="mt-3 block border border-forensics-border bg-forensics-input-bg p-3">
        <span className="block text-[11px] font-semibold text-forensics-text-tertiary">{t('settings.sections.importPerformance.importAnalysisMode')}</span>
        <Select
          value={importAnalysisMode}
          onValueChange={(value) =>
            setSettings((current) => ({
              ...current,
              importAnalysisMode:
                value === 'fullContent'
                  ? 'fullContent'
                  : value === 'budgetedContent'
                    ? 'budgetedContent'
                    : 'metadataOnly',
            }))
          }
        >
          <SelectTrigger variant="forensics" size="sm" className="mt-2">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="metadataOnly">{t('settings.sections.importPerformance.metadataOnly')}</SelectItem>
            <SelectItem value="budgetedContent">{t('settings.sections.importPerformance.budgetedContent')}</SelectItem>
            <SelectItem value="fullContent">{t('settings.sections.importPerformance.fullContent')}</SelectItem>
          </SelectContent>
        </Select>
        <FieldHint className="mt-1 text-[10px] text-forensics-muted-lighter">
          {t('settings.sections.importPerformance.importAnalysisModeHint')}
        </FieldHint>
      </label>
    </section>
  );
}
