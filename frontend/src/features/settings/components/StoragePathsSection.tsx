import { useTranslation } from 'react-i18next';
import { Field, FieldHint, FieldLabel } from '@/app/components/ui/field';
import { Input } from '@/app/components/ui/input';
import type { LocalSettings } from '@/lib/settings';

interface StoragePathsSectionProps {
  caseRoot: string;
  imageSearchPaths: string;
  setSettings: React.Dispatch<React.SetStateAction<LocalSettings>>;
}

export function StoragePathsSection({ caseRoot, imageSearchPaths, setSettings }: StoragePathsSectionProps) {
  const { t } = useTranslation();
  return (
    <div className="max-w-4xl space-y-5">
      <Field layout="row">
        <FieldLabel htmlFor="settings-case-root">{t('settings.sections.storagePaths.caseRoot')}</FieldLabel>
        <Input id="settings-case-root" value={caseRoot} variant="path"
          onChange={(event) => setSettings((current) => ({ ...current, caseRoot: event.target.value }))} />
        <FieldHint>{t('settings.sections.storagePaths.caseRootHint')}</FieldHint>
      </Field>
      <Field layout="row">
        <FieldLabel htmlFor="settings-image-search-paths">{t('settings.sections.storagePaths.imageSearchPaths')}</FieldLabel>
        <Input id="settings-image-search-paths" value={imageSearchPaths} variant="path"
          onChange={(event) => setSettings((current) => ({ ...current, imageSearchPaths: event.target.value }))} />
        <FieldHint>{t('settings.sections.storagePaths.imageSearchPathsHint')}</FieldHint>
      </Field>
    </div>
  );
}
