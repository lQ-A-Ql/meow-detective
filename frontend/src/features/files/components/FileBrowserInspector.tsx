import { useTranslation } from 'react-i18next';
import { Button } from '@/app/components/ui/button';
import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import type { FileEntryRow, FileTreeNode } from '@/types/models';
import type { DataSourcePartition } from '@/types/models';
import { BitLockerVolumePanel } from '@/features/files/components/BitLockerVolumePanel';
import type { BitLockerVolumeModel } from '@/features/files/hooks/use-bitlocker-volume';

interface FileBrowserInspectorProps {
  selectedFile?: FileEntryRow;
  activeDirectoryPath?: string;
  currentDirectory?: FileTreeNode;
  onExtractFile: (file: FileEntryRow) => void;
  extractionPending: boolean;
  onViewTimeline: () => void;
  bitLockerPartition?: DataSourcePartition;
  bitLocker?: BitLockerVolumeModel;
}

export function FileBrowserInspector({
  selectedFile,
  activeDirectoryPath,
  currentDirectory,
  onExtractFile,
  extractionPending,
  onViewTimeline,
  bitLockerPartition,
  bitLocker,
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
        {bitLockerPartition && bitLocker ? (
          <BitLockerVolumePanel partition={bitLockerPartition} model={bitLocker} />
        ) : null}
        <InspectorSection title={t('fileBrowser.inspector.sections.identity')}>
          <InspectorValue value={selectedFile?.name ?? '-'} mono strong />
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
          <InspectorValue value={selectedFile?.hashSha256 ?? '-'} mono />
        </InspectorSection>

        <InspectorSection title={t('fileBrowser.inspector.sections.status')}>
          <div className="font-mono text-[11px] grid grid-cols-[60px_1fr] gap-1">
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.status.deleted')}</div>
            <div className="text-forensics-text-secondary">{selectedFile?.deleted ? 'true' : 'false'}</div>
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.status.hidden')}</div>
            <div className="text-forensics-text-secondary">{selectedFile?.hidden ? 'true' : 'false'}</div>
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.status.system')}</div>
            <div className="text-forensics-text-secondary">{selectedFile?.system ? 'true' : 'false'}</div>
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.status.readOnly')}</div>
            <div className="text-forensics-text-secondary">{selectedFile?.readOnly ? 'true' : 'false'}</div>
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.status.archive')}</div>
            <div className="text-forensics-text-secondary">{selectedFile?.archive ? 'true' : 'false'}</div>
          </div>
        </InspectorSection>

        <InspectorSection title={t('fileBrowser.inspector.sections.actions')}>
          <div className="flex flex-col gap-2">
            <Button
              type="button"
              variant="forensicsSurface"
              size="xs"
              onClick={() => {
                if (selectedFile) {
                  onExtractFile(selectedFile);
                }
              }}
              disabled={!selectedFile || extractionPending}
              className="w-full font-light"
            >
              {extractionPending
                ? t('fileBrowser.inspector.extract.pending')
                : t('fileBrowser.inspector.extract.button')}
            </Button>
            <Button
              type="button"
              variant="forensicsLink"
              size="xs"
              onClick={onViewTimeline}
              className="w-full"
            >
              {t('fileBrowser.inspector.viewTimeline')}
            </Button>
          </div>
        </InspectorSection>
      </div>
    </InspectorPane>
  );
}
