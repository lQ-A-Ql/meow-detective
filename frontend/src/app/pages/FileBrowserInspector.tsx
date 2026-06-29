import { useTranslation } from 'react-i18next';
import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import type { FileEntryRow, FileTreeNode } from '@/types/models';

interface FileBrowserInspectorProps {
  selectedFile?: FileEntryRow;
  activeDirectoryPath?: string;
  currentDirectory?: FileTreeNode;
  extractFile: {
    mutate: (file: FileEntryRow) => void;
    isPending: boolean;
  };
  onViewTimeline: () => void;
}

export function FileBrowserInspector({
  selectedFile,
  activeDirectoryPath,
  currentDirectory,
  extractFile,
  onViewTimeline,
}: FileBrowserInspectorProps) {
  const { t } = useTranslation();

  return (
    <InspectorPane
      className="hidden lg:flex"
      title={t('fileBrowser.inspector.title')}
      subtitle={
        selectedFile
          ? t('fileBrowser.inspector.selected', { name: selectedFile.name })
          : t('fileBrowser.inspector.noSelection')
      }
      widthClassName="w-80"
    >
      <div className="space-y-5">
        <InspectorSection title={t('fileBrowser.inspector.sections.identity')}>
          <InspectorValue
            value={selectedFile?.name ?? '-'}
            mono
            strong
          />
        </InspectorSection>

        <InspectorSection title={t('fileBrowser.inspector.sections.path')}>
          <InspectorValue
            value={
              selectedFile?.path ??
              activeDirectoryPath ??
              currentDirectory?.name ??
              '-'
            }
            mono
          />
        </InspectorSection>

        <InspectorSection title={t('fileBrowser.inspector.sections.timestamps')}>
          <div className="font-mono text-[11px] grid grid-cols-[30px_1fr] gap-1">
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.timestamp.modified')}</div>
            <div className="text-forensics-text-secondary">
              {selectedFile?.modifiedAt ?? '-'}
            </div>
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.timestamp.accessed')}</div>
            <div className="text-forensics-text-secondary">
              {selectedFile?.accessedAt ?? '-'}
            </div>
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.timestamp.changed')}</div>
            <div className="text-forensics-text-secondary">
              {selectedFile?.changedAt ?? '-'}
            </div>
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.timestamp.created')}</div>
            <div className="text-forensics-text-secondary">
              {selectedFile?.createdAt ?? '-'}
            </div>
          </div>
        </InspectorSection>

        <InspectorSection title={t('fileBrowser.inspector.sections.summary')}>
          <InspectorValue
            value={selectedFile?.hashSha256 ?? '-'}
            mono
          />
        </InspectorSection>

        <InspectorSection title={t('fileBrowser.inspector.sections.status')}>
          <div className="font-mono text-[11px] grid grid-cols-[60px_1fr] gap-1">
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.status.deleted')}</div>
            <div className="text-forensics-text-secondary">{selectedFile?.deleted ? 'true' : 'false'}</div>
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.status.hidden')}</div>
            <div className="text-forensics-text-secondary">{selectedFile?.hidden ? 'true' : 'false'}</div>
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.status.system')}</div>
            <div className="text-forensics-text-secondary">{selectedFile?.system ? 'true' : 'false'}</div>
          </div>
        </InspectorSection>

        <InspectorSection title={t('fileBrowser.inspector.sections.actions')}>
          <div className="flex flex-col gap-2">
            <button
              type="button"
              onClick={() => {
                if (selectedFile) {
                  extractFile.mutate(selectedFile);
                }
              }}
              disabled={!selectedFile || extractFile.isPending}
              className="w-full border border-forensics-border-strong bg-forensics-surface text-forensics-text hover:bg-forensics-hover py-1.5 text-center text-[11px] rounded-[2px] cursor-pointer font-medium disabled:opacity-50"
            >
              {extractFile.isPending
                ? t('fileBrowser.inspector.extract.pending')
                : t('fileBrowser.inspector.extract.button')}
            </button>
            <button
              onClick={onViewTimeline}
              className="w-full border border-transparent text-forensics-muted hover:text-forensics-text py-1.5 text-center text-[11px] cursor-pointer underline hover:no-underline"
            >
              {t('fileBrowser.inspector.viewTimeline')}
            </button>
          </div>
        </InspectorSection>
      </div>
    </InspectorPane>
  );
}
