import { useTranslation } from 'react-i18next';
import { Field, FieldHint, FieldLabel } from '@/app/components/ui/field';
import { Input } from '@/app/components/ui/input';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/app/components/ui/select';
import type { LocalSettings } from '@/lib/settings';

interface Props {
  maxImportWorkers: string;
  maxAnalysisWorkers: string;
  importAnalysisMode: LocalSettings['importAnalysisMode'];
  setSettings: React.Dispatch<React.SetStateAction<LocalSettings>>;
}

export function ImportPerformanceSection({ maxImportWorkers, maxAnalysisWorkers, importAnalysisMode, setSettings }: Props) {
  const { t } = useTranslation();
  return (
    <div className="max-w-4xl space-y-5">
      <h2 className="mb-5 text-[14px] text-forensics-ink">{t('settings.sections.importPerformance.title')}</h2>
      <Field layout="row">
        <FieldLabel htmlFor="settings-import-workers">{t('settings.sections.importPerformance.maxImportWorkers')}</FieldLabel>
        <Input id="settings-import-workers" value={maxImportWorkers} inputMode="numeric" variant="numeric"
          onChange={(event) => setSettings((current) => ({ ...current, maxImportWorkers: event.target.value }))} />
        <FieldHint>{t('settings.sections.importPerformance.maxImportWorkersHint')}</FieldHint>
      </Field>
      <Field layout="row">
        <FieldLabel htmlFor="settings-analysis-workers">{t('settings.sections.importPerformance.maxAnalysisWorkers')}</FieldLabel>
        <Input id="settings-analysis-workers" value={maxAnalysisWorkers} inputMode="numeric" variant="numeric"
          onChange={(event) => setSettings((current) => ({ ...current, maxAnalysisWorkers: event.target.value }))} />
        <FieldHint>{t('settings.sections.importPerformance.maxAnalysisWorkersHint')}</FieldHint>
      </Field>
      <Field layout="row">
        <FieldLabel htmlFor="settings-import-mode">{t('settings.sections.importPerformance.importAnalysisMode')}</FieldLabel>
        <Select value={importAnalysisMode} onValueChange={(value) => setSettings((current) => ({ ...current,
          importAnalysisMode: value === 'fullContent' ? 'fullContent' : value === 'budgetedContent' ? 'budgetedContent' : 'metadataOnly' }))}>
          <SelectTrigger id="settings-import-mode"><SelectValue /></SelectTrigger>
          <SelectContent>
            <SelectItem value="metadataOnly">{t('settings.sections.importPerformance.metadataOnly')}</SelectItem>
            <SelectItem value="budgetedContent">{t('settings.sections.importPerformance.budgetedContent')}</SelectItem>
            <SelectItem value="fullContent">{t('settings.sections.importPerformance.fullContent')}</SelectItem>
          </SelectContent>
        </Select>
        <FieldHint>{t('settings.sections.importPerformance.importAnalysisModeHint')}</FieldHint>
      </Field>
    </div>
  );
}
