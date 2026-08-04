import { ChevronRight, HardDrive } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Input } from '@/app/components/ui/input';
import { InteractionLock } from '@/components/layout/InteractionLock';
import { PageSubbar } from '@/components/layout/PageSubbar';
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
      <InteractionLock className="flex min-h-0 flex-1 flex-col" locked={model.bitLocker.importing}>
        <PageSubbar title={t('fileBrowser.subbar.title')} meta={t('fileBrowser.subbar.meta', { directoryCount: model.sortedRows.length, executableCount: model.executableCount })}>
          <div className="flex h-10 min-w-0 shrink-0 items-center gap-4 overflow-x-auto px-4 text-xs">
            <div className="flex min-w-0 items-center gap-1.5 font-mono text-[11px] text-forensics-muted"><HardDrive size={12} />
              {model.treeNodes.length > 0 ? <>
                <span className="max-w-[200px] truncate font-light text-forensics-text">{model.activeRootNode ? model.displayNodeName(model.activeRootNode.name, model.activeRootNode.depth, model.activeRootNode.dataSourceId) : '/'}</span>
                {model.currentDirectory && model.currentDirectory.id !== model.activeRootNode?.id ? <><ChevronRight size={12} className="text-forensics-500" /><span className="max-w-[200px] truncate font-light text-forensics-text">{model.displayNodeName(model.currentDirectory.name, model.currentDirectory.depth, model.currentDirectory.dataSourceId)}</span></> : null}
                {model.selectedFile ? <><ChevronRight size={12} className="text-forensics-500" /><span className="max-w-[200px] truncate font-light text-forensics-text">{model.selectedFile.name}</span></> : null}
              </> : <span className="text-forensics-500">{t('fileBrowser.breadcrumb.noData')}</span>}
            </div>
            <div className="h-4 border-l border-forensics-border" />
            <div className="flex items-center gap-2 text-forensics-muted">{t('fileBrowser.filter.label')}<Input type="text" variant="mono" inputSize="inline" className="w-40 bg-forensics-surface focus-visible:border-forensics-muted" defaultValue={t('fileBrowser.filter.placeholder')} /></div>
            <div className="font-mono text-[11px] text-forensics-muted-light">{t('fileBrowser.viewer.status')}</div>
            <FileVisibilityToggle checked={model.showHidden} onCheckedChange={model.setShowHidden} />
            <ImageMountControl model={mountModel} />
            <div className="ml-auto text-[11px] text-forensics-muted-light">{model.rowsPage?.truncated ? t('fileBrowser.itemCountWithTotal', { visibleCount: model.sortedRows.length, total: model.rowsPage.totalCount }) : t('fileBrowser.itemCount', { visibleCount: model.sortedRows.length })}</div>
          </div>
        </PageSubbar>
        <div className="flex min-h-0 flex-1 overflow-hidden">
          <FileTreePanel filteredTreeNodes={model.filteredTreeNodes} activeDirectoryId={model.activeDirectoryId} expandedIds={model.expandedIdSet} activeChildrenPage={model.activeChildrenPage} activeTreeChildrenLoaded={model.activeTreeChildrenLoaded} canLoadMoreTreeChildren={model.canLoadMoreTreeChildren} loadMoreActiveTreeChildren={model.loadMoreActiveTreeChildren} toggleDirectory={model.toggleDirectory} displayNodeName={model.displayNodeName} filterQuery={model.filterQuery} setFilterQuery={model.setFilterQuery} treeWidth={model.treeWidth} dataSources={model.dataSources} isResizing={model.isResizing} onResizeStart={model.onResizeStart} treeContainerRef={model.treeContainerRef} FILE_BROWSER_PAGE_LIMIT={model.FILE_BROWSER_PAGE_LIMIT} />
          <div className="flex min-h-0 min-w-0 flex-1 flex-col"><FileListPanel sortedRows={model.sortedRows} selectedFileId={model.selectedFileId} viewerTab={model.viewerTab} fileSortKey={model.fileSortKey} fileSortDirection={model.fileSortDirection} handleSort={model.handleSort} setSelectedDirectoryId={model.setSelectedDirectoryId} setSelectedFileId={model.setSelectedFileId} setExpandedDirectoryIds={model.setExpandedDirectoryIds} parentDirectory={model.parentDirectory} goToParentDirectory={model.goToParentDirectory} rowsPage={model.rowsPage} canGoToPreviousRows={model.canGoToPreviousRows} canGoToNextRows={model.canGoToNextRows} goToPreviousRows={model.goToPreviousRows} goToNextRows={model.goToNextRows} onExtractFile={model.fileExtraction.openForFile} />
            <FilePreviewPanel viewerTab={model.viewerTab} setViewerTab={model.setViewerTab} viewer={model.hexPreviewEnabled ? model.viewer : undefined} fileHandle={model.fileHandle} previewKind={model.previewKind} onHexJumpInputChange={model.setJumpOffsetInput} onHexJump={model.jumpToOffset} onLoadNextHexRange={model.loadNextRange} onLoadPreviousHexRange={model.loadPreviousRange} textPreview={model.textPreview} imagePreview={model.imagePreview} mediaUrl={model.mediaUrl} documentPreview={model.documentPreview} selectedFile={model.selectedFile} previewError={model.previewError} onRetryPreview={model.onRetryPreview} />
          </div>
          <FileBrowserInspector selectedFile={model.selectedFile} activeDirectoryPath={model.activeDirectoryPath} currentDirectory={model.currentDirectory} onExtractFile={model.fileExtraction.openForFile} extractionPending={model.fileExtraction.isExtracting} onViewTimeline={model.onViewTimeline} bitLockerPartition={model.bitLockerPartition} bitLocker={model.bitLocker} />
        </div>
      </InteractionLock>
      {model.bitLocker.catalogImport ? <BitLockerCatalogImportOverlay lifecycle={model.bitLocker.catalogImport} /> : null}
      <FileExtractionDialogs model={model.fileExtraction} />
    </div>
  );
}
