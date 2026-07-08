import { useTranslation } from 'react-i18next';
import { FolderOpen, HardDrive } from 'lucide-react';
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
    <>
      <section>
        <div className="flex items-center gap-2 mb-3">
          <FolderOpen size={14} className="text-forensics-muted-light" />
          <FieldLabel htmlFor="settings-case-root" className="text-[13px] font-semibold">
            {t('settings.sections.storagePaths.caseRoot')}
          </FieldLabel>
        </div>
        <Field className="max-w-3xl">
        <Input
          id="settings-case-root"
          value={caseRoot}
          onChange={(event) =>
            setSettings((current) => ({ ...current, caseRoot: event.target.value }))
          }
          variant="path"
        />
        <FieldHint className="text-[10px] text-forensics-muted-lighter">
          {t('settings.sections.storagePaths.caseRootHint')}
        </FieldHint>
        </Field>
      </section>

      <section>
        <div className="flex items-center gap-2 mb-3">
          <HardDrive size={14} className="text-forensics-muted-light" />
          <FieldLabel htmlFor="settings-image-search-paths" className="text-[13px] font-semibold">
            {t('settings.sections.storagePaths.imageSearchPaths')}
          </FieldLabel>
        </div>
        <Field className="max-w-3xl">
        <Input
          id="settings-image-search-paths"
          value={imageSearchPaths}
          onChange={(event) =>
            setSettings((current) => ({ ...current, imageSearchPaths: event.target.value }))
          }
          variant="path"
        />
        <FieldHint className="text-[10px] text-forensics-muted-lighter">
          {t('settings.sections.storagePaths.imageSearchPathsHint')}
        </FieldHint>
        </Field>
      </section>
    </>
  );
}
