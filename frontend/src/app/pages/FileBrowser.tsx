import { ChevronRight, HardDrive } from 'lucide-react';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { FileVisibilityToggle } from '@/components/files/FileVisibilityToggle';
import { FileTreePanel } from './FileTreePanel';
import { FileListPanel } from './FileListPanel';
import { FilePreviewPanel } from './FilePreviewPanel';
import { FileBrowserInspector } from './FileBrowserInspector';
import { useFileBrowser } from './use-file-browser';

export function FileBrowser() {
  const fb = useFileBrowser();

  if (!fb.currentCase) {
    return (
      <div className="flex-1 flex items-center justify-center bg-white">
        <div className="w-full max-w-xl border border-[#e0e0e0] bg-[#fafafa] p-8 text-center">
          <div className="font-serif text-2xl text-[#111] mb-3">
            文件浏览待激活
          </div>
          <div className="text-[13px] text-[#666] leading-6">
            先在案件概览页创建或打开案件，再导入镜像或逻辑目录，即可在这里浏览目录树和文件内容。
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col w-full h-full bg-white min-w-0">
      <PageSubbar
        title="文件浏览控制"
        meta={`当前目录 ${fb.sortedRows.length} 项 / 可执行对象 ${fb.executableCount} 项`}
      >
        <div className="h-10 flex items-center px-4 gap-4 text-xs shrink-0">
          <div className="flex items-center gap-1.5 text-[#666] font-mono text-[11px] min-w-0">
            <HardDrive size={12} />
            {fb.treeNodes.length > 0 ? (
              <>
                <span className="text-[#111] font-semibold truncate max-w-[200px]">
                  {fb.activeRootNode
                    ? fb.displayNodeName(fb.activeRootNode.name, fb.activeRootNode.depth)
                    : '/'}
                </span>
                {fb.currentDirectory &&
                fb.currentDirectory.id !== fb.activeRootNode?.id ? (
                  <>
                    <ChevronRight size={12} className="text-[#aaa]" />
                    <span className="text-[#111] font-semibold truncate max-w-[200px]">
                      {fb.displayNodeName(fb.currentDirectory.name, fb.currentDirectory.depth)}
                    </span>
                  </>
                ) : null}
                {fb.selectedFile ? (
                  <>
                    <ChevronRight size={12} className="text-[#aaa]" />
                    <span className="text-[#111] font-semibold truncate max-w-[200px]">
                      {fb.selectedFile.name}
                    </span>
                  </>
                ) : null}
              </>
            ) : (
              <span className="text-[#aaa]">无数据源</span>
            )}
          </div>
          <div className="h-4 border-l border-[#e0e0e0]" />
          <div className="text-[#666] flex items-center gap-2">
            过滤:
            <input
              type="text"
              className="bg-white border border-[#ccc] px-2 py-0.5 text-[#111] font-mono text-[11px] rounded-[2px] outline-none w-40 focus:border-[#666]"
              defaultValue="*"
            />
          </div>
          <div className="text-[11px] text-[#888] font-mono">
            viewer: metadata / hex 已启用
          </div>
          <FileVisibilityToggle checked={fb.showHidden} onCheckedChange={fb.setShowHidden} />
          <div className="ml-auto text-[#888] text-[11px]">
            显示 {fb.sortedRows.length}
            {fb.rowsPage?.truncated ? ` / ${fb.rowsPage.totalCount}` : ''} 个项目
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
