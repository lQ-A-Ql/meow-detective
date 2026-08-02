import { useEffect, useMemo, useState } from 'react';
import { useNavigate } from 'react-router';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { useFileJumpContext } from '@/features/files/hooks';
import { collectTreeNodeIds } from '@/features/files/file-tree-utils';
import { useFilePagination } from '@/features/files/hooks/use-file-pagination';
import { useFilePreview } from '@/features/files/hooks/use-file-preview';
import { useFileSelection } from '@/features/files/hooks/use-file-selection';
import { useFileTree } from '@/features/files/hooks/use-file-tree';
import { isBitLockerPartition, type BitLockerTarget } from '@/features/files/bitlocker';
import { useBitLockerVolumeModel } from '@/features/files/hooks/use-bitlocker-volume';
import { useFileExtractionModel } from '@/features/files/hooks/use-file-extraction';
import { useUiStore } from '@/stores/ui-store';
import { partitionIndexFromRootName } from '@/lib/partition-display';
import type { DataSourcePartition } from '@/types/models';

const FILE_BROWSER_PAGE_LIMIT = 500;

function findActivePartitionNode(
  nodes: ReturnType<typeof useFileTree>['treeNodes'],
  activeDirectoryId?: string,
) {
  const activeIndex = nodes.findIndex((node) => node.id === activeDirectoryId);
  if (activeIndex < 0) {
    return undefined;
  }
  for (let index = activeIndex; index >= 0; index -= 1) {
    const node = nodes[index];
    if (node.depth === 1 && node.dataSourceId) {
      return node;
    }
  }
  return undefined;
}

export function useFileBrowserModel() {
  const navigate = useNavigate();
  const { data: currentCase } = useCurrentCase();
  const { data: dataSources } = useDataSources();
  const viewerTab = useUiStore((s) => s.viewerTab);
  const setViewerTab = useUiStore((s) => s.setViewerTab);
  const [showHidden, setShowHidden] = useState(true);

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
    dataSources,
  });

  const pagination = useFilePagination({
    activeDirectoryId: tree.activeDirectoryIsDataSource ? undefined : tree.activeDirectoryId,
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
    pagination.fileSortDirection,
  );

  useEffect(() => {
    const visibleIds = collectTreeNodeIds(tree.rootTree, tree.treeChildren);
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

  const activePartitionContext = useMemo(() => {
    const node = findActivePartitionNode(tree.treeNodes, tree.activeDirectoryId);
    const partitionIndex = node ? partitionIndexFromRootName(node.name) : undefined;
    if (!node?.dataSourceId || partitionIndex === undefined) {
      return undefined;
    }
    const source = dataSources?.find((item) => item.id === node.dataSourceId);
    const partition = source?.partitions?.find((item) => item.index === partitionIndex);
    return source && partition
      ? { dataSourceId: source.id, partition }
      : undefined;
  }, [dataSources, tree.activeDirectoryId, tree.treeNodes]);

  const activePartition = activePartitionContext?.partition;

  const bitLockerTarget = useMemo<BitLockerTarget | undefined>(() => {
    if (!activePartitionContext || !isBitLockerPartition(activePartitionContext.partition)) {
      return undefined;
    }
    return {
      dataSourceId: activePartitionContext.dataSourceId,
      partitionIndex: activePartitionContext.partition.index,
    };
  }, [activePartitionContext]);
  const bitLocker = useBitLockerVolumeModel(bitLockerTarget);
  const fileExtraction = useFileExtractionModel();

  const preview = useFilePreview({
    selectedFile,
    viewerTab,
  });

  const onViewTimeline = () => {
    if (!selectedFile) return;
    setSelectedTimelineId(selectedFile.id);
    navigate('/timeline');
  };

  return {
    currentCase,
    treeWidth: tree.treeWidth,
    isResizing: tree.isResizing,
    onResizeStart: tree.onResizeStart,
    filteredTreeNodes: tree.filteredTreeNodes,
    activeDirectoryId: tree.activeDirectoryId,
    expandedIdSet: tree.expandedIdSet,
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
    documentPreview: preview.documentPreview,
    previewError: preview.previewError,
    onRetryPreview: preview.onRetryPreview,
    selectedFile,
    activePartition,
    bitLockerPartition: bitLockerTarget ? activePartition : undefined,
    bitLocker,
    activeDirectoryPath: pagination.activeDirectoryPath,
    currentDirectory: tree.currentDirectory,
    parentDirectory: tree.parentDirectory,
    goToParentDirectory: tree.goToParentDirectory,
    activeRootNode: tree.activeRootNode,
    executableCount: pagination.executableCount,
    fileExtraction,
    onViewTimeline,
    dataSources,
  };
}
