import { useTranslation } from 'react-i18next';
import { Download, History } from 'lucide-react';
import { IconButton } from '@/components/controls/IconButton';

import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import type { FileEntryRow, FileTreeNode } from '@/types/models';
import type { DataSourcePartition } from '@/types/models';
import { BitLockerVolumePanel } from '@/features/files/components/BitLockerVolumePanel';
import { formatUnixMode } from '@/features/files/components/FileListPanel';
import type { BitLockerVolumeModel } from '@/features/files/hooks/use-bitlocker-volume';
import type { ApiErrorDto, ImageMetadata } from '@/types/models';
import { ImageMetadataInspector } from '@/features/files/components/ImageMetadataInspector';
import { NtfsTechnicalInspector } from '@/features/files/components/NtfsTechnicalInspector';
import { FileDigestInspector } from '@/features/files/components/FileDigestInspector';
import type { DigestAlgorithm, EvidenceDigest } from '@/types/digest';
import type { NtfsTechnicalFile } from '@/types/files';
import type { ntfsInspectorRuns } from '../ntfs-inspector-runs';

interface FileBrowserInspectorProps {
  selectedFile?: FileEntryRow;
  activeDirectoryPath?: string;
  currentDirectory?: FileTreeNode;
  onExtractFile: (file: FileEntryRow) => void;
  extractionPending: boolean;
  onViewTimeline: () => void;
  bitLockerPartition?: DataSourcePartition;
  bitLocker?: BitLockerVolumeModel;
  imageMetadata?: ImageMetadata | null;
  imageMetadataLoading?: boolean;
  imageMetadataError?: ApiErrorDto | null;
  onRetryImageMetadata?: () => void;
  ntfsTechnical?: NtfsTechnicalFile;
  ntfsTechnicalEligible?: boolean;
  ntfsTechnicalLoading?: boolean;
  ntfsTechnicalErrorText?: string;
  ntfsTechnicalUnsupported?: boolean;
  ntfsTechnicalRuns?: ReturnType<typeof ntfsInspectorRuns>;
  retryNtfsTechnical?: () => void;
  fileDigest?: EvidenceDigest;
  fileDigestLoading?: boolean;
  fileDigestErrorText?: string;
  fileDigestAvailable?: boolean;
  calculateFileDigest: (algorithm: DigestAlgorithm) => void;
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
  imageMetadata,
  imageMetadataLoading,
  imageMetadataError,
  onRetryImageMetadata,
  ntfsTechnical,
  ntfsTechnicalEligible,
  ntfsTechnicalLoading,
  ntfsTechnicalErrorText,
  ntfsTechnicalUnsupported,
  ntfsTechnicalRuns,
  retryNtfsTechnical,
  fileDigest,
  fileDigestLoading,
  fileDigestErrorText,
  fileDigestAvailable,
  calculateFileDigest,
}: FileBrowserInspectorProps) {
  const { t } = useTranslation();

  return (
    <InspectorPane
      className="hidden lg:flex"
      kind={t('inspectorKind.file')}
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

        <FileDigestInspector digest={fileDigest} loading={Boolean(fileDigestLoading)} error={fileDigestErrorText} available={Boolean(fileDigestAvailable)} onCalculate={calculateFileDigest} />
        <NtfsTechnicalInspector technical={ntfsTechnical} eligible={ntfsTechnicalEligible} loading={ntfsTechnicalLoading} errorText={ntfsTechnicalErrorText} unsupported={ntfsTechnicalUnsupported} runs={ntfsTechnicalRuns} onRetry={retryNtfsTechnical} />

        {selectedFile?.entryType === 'file' && (imageMetadata || imageMetadataLoading || imageMetadataError) ? (
          <ImageMetadataInspector
            metadata={imageMetadata}
            loading={Boolean(imageMetadataLoading)}
            error={imageMetadataError}
            onRetry={onRetryImageMetadata}
          />
        ) : null}

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
            <div className="text-forensics-muted-light">{t('fileBrowser.inspector.status.permissions')}</div>
            <div className="text-forensics-text-secondary">
              {selectedFile?.unixMode !== undefined && selectedFile?.unixMode !== null
                ? `${(selectedFile.unixMode & 0o7777).toString(8).padStart(4, '0')} ${formatUnixMode(selectedFile.unixMode)}`
                : '-'}
            </div>
          </div>
        </InspectorSection>

        <InspectorSection title={t('fileBrowser.inspector.sections.actions')}>
          <div className="flex items-center gap-2">
            <IconButton
              label={
                extractionPending
                  ? t('fileBrowser.inspector.extract.pending')
                  : t('fileBrowser.inspector.extract.button')
              }
              variant="forensicsSurface"
              onClick={() => {
                if (selectedFile) {
                  onExtractFile(selectedFile);
                }
              }}
              disabled={!selectedFile || extractionPending}
            >
              <Download size={14} />
            </IconButton>
            <IconButton label={t('fileBrowser.inspector.viewTimeline')} onClick={onViewTimeline}>
              <History size={14} />
            </IconButton>
          </div>
        </InspectorSection>
      </div>
    </InspectorPane>
  );
}
