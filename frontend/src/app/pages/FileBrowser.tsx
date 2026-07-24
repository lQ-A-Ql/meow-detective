import { ChevronRight, HardDrive } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Input } from '@/app/components/ui/input';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { FileVisibilityToggle } from '@/features/files/components/FileVisibilityToggle';
import { FileTreePanel } from '@/features/files/components/FileTreePanel';
import { FileListPanel } from '@/features/files/components/FileListPanel';
import { FilePreviewPanel } from '@/features/files/components/FilePreviewPanel';
import { FileBrowserInspector } from '@/features/files/components/FileBrowserInspector';
import { useFileBrowserModel } from '@/features/files/use-file-browser-model';

export function FileBrowser() {
  const { t } = useTranslation();
  const fb = useFileBrowserModel();

  if (!fb.currentCase) {
    return (
      <div className="flex-1 flex items-center justify-center bg-forensics-surface">
        <div className="w-full max-w-xl border border-forensics-border bg-forensics-panel p-8 text-center">
          <div className="font-serif text-2xl text-forensics-text mb-3">
            {t('fileBrowser.empty.title')}
          </div>
          <div className="text-[13px] text-forensics-muted leading-6">
            {t('fileBrowser.empty.description')}
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-forensics-surface min-w-0">
      <PageSubbar
        title={t('fileBrowser.subbar.title')}
        meta={t('fileBrowser.subbar.meta', {
          directoryCount: fb.sortedRows.length,
          executableCount: fb.executableCount,
        })}
      >
        <div className="h-10 flex items-center px-4 gap-4 text-xs shrink-0 min-w-0 overflow-x-auto">
          <div className="flex items-center gap-1.5 text-forensics-muted font-mono text-[11px] min-w-0">
            <HardDrive size={12} />
            {fb.treeNodes.length > 0 ? (
              <>
                <span className="text-forensics-text font-light truncate max-w-[200px]">
                  {fb.activeRootNode
                    ? fb.displayNodeName(fb.activeRootNode.name, fb.activeRootNode.depth, fb.activeRootNode.dataSourceId)
                    : '/'}
                </span>
                {fb.currentDirectory &&
                fb.currentDirectory.id !== fb.activeRootNode?.id ? (
                  <>
                    <ChevronRight size={12} className="text-forensics-500" />
                    <span className="text-forensics-text font-light truncate max-w-[200px]">
                      {fb.displayNodeName(fb.currentDirectory.name, fb.currentDirectory.depth, fb.currentDirectory.dataSourceId)}
                    </span>
                  </>
                ) : null}
                {fb.selectedFile ? (
                  <>
                    <ChevronRight size={12} className="text-forensics-500" />
                    <span className="text-forensics-text font-light truncate max-w-[200px]">
                      {fb.selectedFile.name}
                    </span>
                  </>
                ) : null}
              </>
            ) : (
              <span className="text-forensics-500">{t('fileBrowser.breadcrumb.noData')}</span>
            )}
          </div>
          <div className="h-4 border-l border-forensics-border" />
          <div className="text-forensics-muted flex items-center gap-2">
            {t('fileBrowser.filter.label')}
            <Input
              type="text"
              variant="mono"
              inputSize="inline"
              className="w-40 bg-forensics-surface focus-visible:border-forensics-muted"
              defaultValue={t('fileBrowser.filter.placeholder')}
            />
          </div>
          <div className="text-[11px] text-forensics-muted-light font-mono">
            {t('fileBrowser.viewer.status')}
          </div>
          <FileVisibilityToggle checked={fb.showHidden} onCheckedChange={fb.setShowHidden} />
          <div className="ml-auto text-forensics-muted-light text-[11px]">
            {fb.rowsPage?.truncated
              ? t('fileBrowser.itemCountWithTotal', {
                  visibleCount: fb.sortedRows.length,
                  total: fb.rowsPage.totalCount,
                })
              : t('fileBrowser.itemCount', { visibleCount: fb.sortedRows.length })}
          </div>
        </div>
      </PageSubbar>

      <div className="flex-1 flex overflow-hidden min-h-0">
        <FileTreePanel
          filteredTreeNodes={fb.filteredTreeNodes}
          activeChildrenPage={fb.activeChildrenPage}
          activeTreeChildrenLoaded={fb.activeTreeChildrenLoaded}
          canLoadMoreTreeChildren={fb.canLoadMoreTreeChildren}
          loadMoreActiveTreeChildren={fb.loadMoreActiveTreeChildren}
          toggleDirectory={fb.toggleDirectory}
          displayNodeName={fb.displayNodeName}
          filterQuery={fb.filterQuery}
          setFilterQuery={fb.setFilterQuery}
          treeWidth={fb.treeWidth}
          dataSources={fb.dataSources}
          isResizing={fb.isResizing}
          onResizeStart={fb.onResizeStart}
          treeContainerRef={fb.treeContainerRef}
          FILE_BROWSER_PAGE_LIMIT={fb.FILE_BROWSER_PAGE_LIMIT}
        />

        <div className="flex-1 flex flex-col min-w-0 min-h-0">
          <FileListPanel
            sortedRows={fb.sortedRows}
            selectedFileId={fb.selectedFileId}
            viewerTab={fb.viewerTab}
            fileSortKey={fb.fileSortKey}
            fileSortDirection={fb.fileSortDirection}
            handleSort={fb.handleSort}
            setSelectedDirectoryId={fb.setSelectedDirectoryId}
            setSelectedFileId={fb.setSelectedFileId}
            setExpandedDirectoryIds={fb.setExpandedDirectoryIds}
            parentDirectory={fb.parentDirectory}
            goToParentDirectory={fb.goToParentDirectory}
            rowsPage={fb.rowsPage}
            canGoToPreviousRows={fb.canGoToPreviousRows}
            canGoToNextRows={fb.canGoToNextRows}
            goToPreviousRows={fb.goToPreviousRows}
            goToNextRows={fb.goToNextRows}
          />

          <FilePreviewPanel
            viewerTab={fb.viewerTab}
            setViewerTab={fb.setViewerTab}
            viewer={fb.hexPreviewEnabled ? fb.viewer : undefined}
            fileHandle={fb.fileHandle}
            previewKind={fb.previewKind}
            onHexJumpInputChange={fb.setJumpOffsetInput}
            onHexJump={fb.jumpToOffset}
            onLoadNextHexRange={fb.loadNextRange}
            onLoadPreviousHexRange={fb.loadPreviousRange}
            textPreview={fb.textPreview}
            imagePreview={fb.imagePreview}
            mediaUrl={fb.mediaUrl}
            documentPreview={fb.documentPreview}
            selectedFile={fb.selectedFile}
          />
        </div>

        <FileBrowserInspector
          selectedFile={fb.selectedFile}
          activeDirectoryPath={fb.activeDirectoryPath}
          currentDirectory={fb.currentDirectory}
          extractFile={fb.extractFile}
          onViewTimeline={fb.onViewTimeline}
        />
      </div>
    </div>
  );
}
