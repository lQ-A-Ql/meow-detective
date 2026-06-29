import { useTranslation } from 'react-i18next';
import { FolderOpen, HardDrive } from 'lucide-react';
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
          <label htmlFor="settings-case-root" className="text-[13px] font-semibold text-forensics-text-secondary">
            {t('settings.sections.storagePaths.caseRoot')}
          </label>
        </div>
        <input
          id="settings-case-root"
          value={caseRoot}
          onChange={(event) =>
            setSettings((current) => ({ ...current, caseRoot: event.target.value }))
          }
          className="w-full max-w-3xl bg-forensics-input-bg border border-forensics-border p-3 font-mono text-[12px] text-forensics-text"
        />
        <div className="mt-1 text-[10px] text-forensics-muted-lighter">
          {t('settings.sections.storagePaths.caseRootHint')}
        </div>
      </section>

      <section>
        <div className="flex items-center gap-2 mb-3">
          <HardDrive size={14} className="text-forensics-muted-light" />
          <label htmlFor="settings-image-search-paths" className="text-[13px] font-semibold text-forensics-text-secondary">
            {t('settings.sections.storagePaths.imageSearchPaths')}
          </label>
        </div>
        <input
          id="settings-image-search-paths"
          value={imageSearchPaths}
          onChange={(event) =>
            setSettings((current) => ({ ...current, imageSearchPaths: event.target.value }))
          }
          className="w-full max-w-3xl bg-forensics-input-bg border border-forensics-border p-3 font-mono text-[12px] text-forensics-text"
        />
        <div className="mt-1 text-[10px] text-forensics-muted-lighter">
          {t('settings.sections.storagePaths.imageSearchPathsHint')}
        </div>
      </section>
    </>
  );
}
