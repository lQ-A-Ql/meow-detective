import { useEffect, useState } from 'react';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { useFileJumpContext } from '@/features/files/hooks';
import { useFilePagination } from '@/features/files/hooks/use-file-pagination';
import { useFilePreview } from '@/features/files/hooks/use-file-preview';
import { useFileSelection } from '@/features/files/hooks/use-file-selection';
import { useFileTree } from '@/features/files/hooks/use-file-tree';
import { useUiStore } from '@/stores/ui-store';
import type { DataSourcePartition } from '@/types/models';

const FILE_BROWSER_PAGE_LIMIT = 500;

export function useFileBrowser() {
  const { data: currentCase } = useCurrentCase();
  const { data: dataSources } = useDataSources();
  const viewerTab = useUiStore((s) => s.viewerTab);
  const setViewerTab = useUiStore((s) => s.setViewerTab);
  const [showHidden, setShowHidden] = useState(false);

  const {
    selectedDirectoryId,
    setSelectedDirectoryId,
    selectedFileId,
    setSelectedFileId,
    setSelectedTimelineId,
  } = useFileSelection();

  const partitions: DataSourcePartition[] =
    dataSources?.flatMap((source) => source.partitions ?? []) ?? [];

  const tree = useFileTree({
    showHidden,
    pageLimit: FILE_BROWSER_PAGE_LIMIT,
    selectedDirectoryId,
    setSelectedDirectoryId,
    setSelectedFileId,
    partitions,
  });

  const pagination = useFilePagination({
    activeDirectoryId: tree.activeDirectoryId,
    pageLimit: FILE_BROWSER_PAGE_LIMIT,
    showHidden,
  });

  const {
    data: jumpContext,
    isLoading: jumpContextLoading,
    isFetching: jumpContextFetching,
  } = useFileJumpContext(
    selectedFileId,
    showHidden,
    FILE_BROWSER_PAGE_LIMIT,
    pagination.fileSortKey,
    pagination.fileSortDirection
  );

  // Clear the selected directory when it drops out of the (possibly filtered) visible tree,
  // e.g. after toggling showHidden off while a hidden directory was selected.
  useEffect(() => {
    const visibleIds = new Set<string>();
    const collect = (nodes: typeof tree.rootTree) => {
      for (const node of nodes ?? []) {
        visibleIds.add(node.id);
        const children = tree.treeChildren[node.id];
        if (children?.length) collect(children);
      }
    };
    collect(tree.rootTree);
    if (jumpContext) {
      visibleIds.add(jumpContext.directory.id);
      for (const id of jumpContext.ancestorDirectoryIds) visibleIds.add(id);
    }
    if (selectedDirectoryId && !visibleIds.has(selectedDirectoryId)) {
      setSelectedDirectoryId(tree.rootTree?.[0]?.id);
      setSelectedFileId(undefined);
    }
  }, [
    jumpContext,
    tree.rootTree,
    tree.treeChildren,
    selectedDirectoryId,
    setSelectedDirectoryId,
    setSelectedFileId,
  ]);

  // Clear the selected file once we're sure it's not reachable in the current row page and
  // there's no pending/available jump context that would bring it into view.
  useEffect(() => {
    if (
      selectedFileId &&
      (!pagination.rows || !pagination.rows.some((row) => row.id === selectedFileId))
    ) {
      if (!jumpContext && !jumpContextLoading && !jumpContextFetching) {
        setSelectedFileId(undefined);
      }
    }
  }, [
    jumpContext,
    jumpContextFetching,
    jumpContextLoading,
    pagination.rows,
    selectedFileId,
    setSelectedFileId,
  ]);

  // Drive tree expansion, directory selection, hidden-visibility, and row offset from the
  // jump context so an off-page (possibly hidden) selected file becomes visible.
  useEffect(() => {
    if (
      !jumpContext ||
      !selectedFileId ||
      jumpContext.target.id !== selectedFileId
    ) {
      return;
    }
    if (jumpContext.requiresShowHidden && !showHidden) {
      setShowHidden(true);
      return;
    }
    if (selectedDirectoryId !== jumpContext.directory.id) {
      setSelectedDirectoryId(jumpContext.directory.id);
    }
    tree.setExpandedDirectoryIds((current) => {
      const next = new Set(current);
      for (const id of jumpContext.ancestorDirectoryIds) next.add(id);
      return next.size === current.length ? current : Array.from(next);
    });
    if (pagination.rowsOffset !== jumpContext.rowOffset) {
      pagination.setRowsOffset(jumpContext.rowOffset);
    }
  }, [
    jumpContext,
    pagination.rowsOffset,
    pagination.setRowsOffset,
    selectedDirectoryId,
    selectedFileId,
    setSelectedDirectoryId,
    showHidden,
    tree.setExpandedDirectoryIds,
  ]);

  const selectedFile = pagination.rows?.find((row) => row.id === selectedFileId);

  const preview = useFilePreview({
    selectedFile,
    viewerTab,
    setSelectedTimelineId,
  });

  return {
    currentCase,
    treeWidth: tree.treeWidth,
    isResizing: tree.isResizing,
    onResizeStart: tree.onResizeStart,
    filteredTreeNodes: tree.filteredTreeNodes,
    activeChildrenPage: tree.activeChildrenPage,
    activeTreeChildrenLoaded: tree.activeTreeChildrenLoaded,
    canLoadMoreTreeChildren: tree.canLoadMoreTreeChildren,
    loadMoreActiveTreeChildren: tree.loadMoreActiveTreeChildren,
    toggleDirectory: tree.toggleDirectory,
    displayNodeName: tree.displayNodeName,
    filterQuery: tree.filterQuery,
    setFilterQuery: tree.setFilterQuery,
    treeContainerRef: tree.treeContainerRef,
    FILE_BROWSER_PAGE_LIMIT,
    showHidden,
    setShowHidden,
    treeNodes: tree.treeNodes,
    sortedRows: pagination.sortedRows,
    selectedFileId,
    viewerTab,
    fileSortKey: pagination.fileSortKey,
    fileSortDirection: pagination.fileSortDirection,
    handleSort: pagination.handleSort,
    setSelectedDirectoryId,
    setSelectedFileId,
    setExpandedDirectoryIds: tree.setExpandedDirectoryIds,
    rowsPage: pagination.rowsPage,
    canGoToPreviousRows: pagination.canGoToPreviousRows,
    canGoToNextRows: pagination.canGoToNextRows,
    goToPreviousRows: pagination.goToPreviousRows,
    goToNextRows: pagination.goToNextRows,
    hexPreviewEnabled: preview.hexPreviewEnabled,
    viewer: preview.viewer,
    fileHandle: preview.fileHandle,
    previewKind: preview.previewKind,
    setViewerTab,
    setJumpOffsetInput: preview.setJumpOffsetInput,
    jumpToOffset: preview.jumpToOffset,
    loadNextRange: preview.loadNextRange,
    loadPreviousRange: preview.loadPreviousRange,
    textPreview: preview.textPreview,
    imagePreview: preview.imagePreview,
    mediaUrl: preview.mediaUrl,
    selectedFile,
    activeDirectoryPath: pagination.activeDirectoryPath,
    currentDirectory: tree.currentDirectory,
    parentDirectory: tree.parentDirectory,
    goToParentDirectory: tree.goToParentDirectory,
    activeRootNode: tree.activeRootNode,
    executableCount: pagination.executableCount,
    extractFile: preview.extractFile,
    onViewTimeline: preview.onViewTimeline,
  };
}

