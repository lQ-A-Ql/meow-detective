import { useTranslation } from 'react-i18next';
import { InteractionLock } from '@/components/layout/InteractionLock';
import { BitLockerCatalogImportOverlay } from '@/features/files/components/BitLockerCatalogImportOverlay';
import { FileBrowserInspector } from '@/features/files/components/FileBrowserInspector';
import { FileExtractionDialogs } from '@/features/files/components/FileExtractionDialogs';
import { FileListPanel } from '@/features/files/components/FileListPanel';
import { FilePreviewPanel } from '@/features/files/components/FilePreviewPanel';
import { FileTreePanel } from '@/features/files/components/FileTreePanel';
import { FileVisibilityToggle } from '@/features/files/components/FileVisibilityToggle';
import { ImageMountControl } from '@/features/files/components/ImageMountControl';
import type { ImageMountModel } from '@/features/files/hooks/use-image-mount-model';
import type { useFileBrowserModel } from '@/features/files/use-file-browser-model';
import { WorkspaceCard } from '@/components/layout/WorkspaceCard';

interface FileBrowserWorkspaceProps {
  model: ReturnType<typeof useFileBrowserModel>;
  mountModel: ImageMountModel;
}

/** Pure file-browser presentation surface. All evidence routing remains in the feature model. */
export function FileBrowserWorkspace({ model, mountModel }: FileBrowserWorkspaceProps) {
  const { t } = useTranslation();

  if (!model.currentCase) {
    return (
      <div className="flex flex-1 items-center justify-center bg-forensics-surface">
        <div className="w-full max-w-xl border border-forensics-border bg-forensics-panel p-8 text-center">
          <div className="mb-3 font-serif text-2xl text-forensics-text">{t('fileBrowser.empty.title')}</div>
          <div className="text-[13px] leading-6 text-forensics-muted">{t('fileBrowser.empty.description')}</div>
        </div>
      </div>
    );
  }

  return (
    <div className="relative flex h-full w-full min-w-0 flex-1 flex-col bg-forensics-surface">
      <InteractionLock className="flex min-h-0 flex-1 gap-[2px] overflow-hidden bg-forensics-base p-[2px]" locked={model.bitLocker.importing}>
        <WorkspaceCard className="shrink-0" style={{ width: `${model.treeWidth}px` }} aria-label={t('fileBrowser.tree.title')}>
          <FileTreePanel filteredTreeNodes={model.filteredTreeNodes} treeLoading={model.treeLoading} activeDirectoryId={model.activeDirectoryId} expandedIds={model.expandedIdSet} activeChildrenPage={model.activeChildrenPage} activeTreeChildrenLoaded={model.activeTreeChildrenLoaded} canLoadMoreTreeChildren={model.canLoadMoreTreeChildren} loadMoreActiveTreeChildren={model.loadMoreActiveTreeChildren} toggleDirectory={model.toggleDirectory} displayNodeName={model.displayNodeName} filterQuery={model.filterQuery} setFilterQuery={model.setFilterQuery} treeWidth={model.treeWidth} dataSources={model.dataSources} isResizing={model.isResizing} onResizeStart={model.onResizeStart} treeContainerRef={model.treeContainerRef} FILE_BROWSER_PAGE_LIMIT={model.FILE_BROWSER_PAGE_LIMIT} />
        </WorkspaceCard>
        <WorkspaceCard className="flex flex-1" aria-label={t('fileBrowser.list.title')}>
        <div className="flex min-h-0 min-w-0 flex-1 flex-col">
          <FileListPanel sortedRows={model.sortedRows} selectedFileId={model.selectedFileId} fileSortKey={model.fileSortKey} fileSortDirection={model.fileSortDirection} handleSort={model.handleSort} setSelectedDirectoryId={model.setSelectedDirectoryId} setSelectedFileId={model.setSelectedFileId} setExpandedDirectoryIds={model.setExpandedDirectoryIds} parentDirectory={model.parentDirectory} goToParentDirectory={model.goToParentDirectory} rowsPage={model.rowsPage} canGoToPreviousRows={model.canGoToPreviousRows} canGoToNextRows={model.canGoToNextRows} goToPreviousRows={model.goToPreviousRows} goToNextRows={model.goToNextRows} onExtractFile={model.fileExtraction.openForFile} visibilityToggle={<FileVisibilityToggle checked={model.showHidden} onCheckedChange={model.setShowHidden} />} mountControl={<ImageMountControl model={mountModel} />} />
          <FilePreviewPanel viewerTab={model.viewerTab} setViewerTab={model.setViewerTab} viewer={model.hexPreviewEnabled ? model.viewer : undefined} fileHandle={model.fileHandle} previewKind={model.previewKind} onHexJumpInputChange={model.setJumpOffsetInput} onHexJump={model.jumpToOffset} onLoadNextHexRange={model.loadNextRange} onLoadPreviousHexRange={model.loadPreviousRange} textPreview={model.textPreview} imagePreview={model.imagePreview} imageMetadata={model.imageMetadata} mediaUrl={model.mediaUrl} documentPreview={model.documentPreview} selectedFile={model.selectedFile} previewError={model.previewError} onRetryPreview={model.onRetryPreview} />
        </div>
        </WorkspaceCard>
        <WorkspaceCard className="flex w-80 shrink-0" aria-label={t('inspectorKind.file')}>
          <FileBrowserInspector selectedFile={model.selectedFile} activeDirectoryPath={model.activeDirectoryPath} currentDirectory={model.currentDirectory} onExtractFile={model.fileExtraction.openForFile} extractionPending={model.fileExtraction.isExtracting} onViewTimeline={model.onViewTimeline} bitLockerPartition={model.bitLockerPartition} bitLocker={model.bitLocker} imageMetadata={model.imageMetadata} imageMetadataLoading={model.imageMetadataLoading} imageMetadataError={model.imageMetadataError} onRetryImageMetadata={model.onRetryImageMetadata} ntfsTechnical={model.ntfsTechnical} fileDigest={model.fileDigest} fileDigestLoading={model.fileDigestLoading} fileDigestError={model.fileDigestError} calculateFileDigest={model.calculateFileDigest} />
        </WorkspaceCard>
      </InteractionLock>
      {model.bitLocker.catalogImport ? <BitLockerCatalogImportOverlay lifecycle={model.bitLocker.catalogImport} /> : null}
      <FileExtractionDialogs model={model.fileExtraction} />
    </div>
  );
}
