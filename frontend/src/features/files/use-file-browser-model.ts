import { useMemo, useState } from 'react';
import { useNavigate } from 'react-router';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { useNtfsTechnicalFile } from '@/features/files/hooks';
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
import { errorMessage } from '@/lib/errors';
import { parseSourceScopedFileId } from './source-scoped-file-id';
import { useNestedEvidenceModel } from './hooks/use-nested-evidence-model';
import { useEvidenceDigestModel } from './hooks/use-evidence-digest-model';
import { useFileBrowserLocation } from './hooks/use-file-browser-location';
import { useFileBrowserCoordinator } from './hooks/use-file-browser-coordinator';

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
  const [showHidden, setShowHiddenState] = useState(true);

  const {
    selectedDirectoryId,
    selectedFileId,
    setSelectedTimelineId,
  } = useFileSelection();

  const partitions: DataSourcePartition[] = useMemo(
    () => dataSources?.flatMap((source) => source.partitions ?? []) ?? [],
    [dataSources],
  );
  const fileSortKey = useUiStore((s) => s.fileSortKey);
  const fileSortDirection = useUiStore((s) => s.fileSortDirection);

  const location = useFileBrowserLocation({ caseId: currentCase?.id, showHidden,
    pageLimit: FILE_BROWSER_PAGE_LIMIT, sortKey: fileSortKey, sortDirection: fileSortDirection });
  const setSelectedDirectoryId = location.selectDirectory;
  const setSelectedFileId = location.selectFile;

  const tree = useFileTree({
    showHidden,
    pageLimit: FILE_BROWSER_PAGE_LIMIT,
    selectedDirectoryId,
    setSelectedDirectoryId,
    setSelectedFileId,
    partitions,
    dataSources,
    caseId: currentCase?.id,
    revealDirectoryIds: location.revealDirectoryIds,
    jumpPathNodes: location.pathNodes,
    deferInitialSelection: location.deferInitialSelection,
  });

  const pagination = useFilePagination({
    activeDirectoryId: tree.activeDirectoryIsDataSource ? undefined : tree.activeDirectoryId,
    pageLimit: FILE_BROWSER_PAGE_LIMIT,
    showHidden,
    caseId: currentCase?.id,
  });

  const { setShowHidden, handleSort, goToPreviousRows, goToNextRows, activateEntry } = useFileBrowserCoordinator({
    caseId: currentCase?.id, selectedDirectoryId, selectedFileId, showHidden,
    onShowHidden: setShowHiddenState, location, tree, pagination,
  });

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
  const nestedEvidence = useNestedEvidenceModel(currentCase?.id);
  const openNestedResult = () => {
    if (nestedEvidence.result) {
      const directoryId = `data-source:${nestedEvidence.result.derivedDataSourceId}`;
      setSelectedDirectoryId(directoryId);
      setSelectedFileId(undefined);
      tree.setExpandedDirectoryIds((current) => Array.from(new Set([...current, directoryId])));
    }
  };

  const preview = useFilePreview({
    selectedFile,
    viewerTab,
    caseId: currentCase?.id,
  });
  const ntfsTechnicalQuery = useNtfsTechnicalFile(currentCase?.id, selectedFile?.id, selectedFile?.entryType === 'file');
  const digestSource = dataSources?.find((source) => source.id === (parseSourceScopedFileId(selectedFile?.id)?.sourceId ?? activePartitionContext?.dataSourceId ?? tree.activeRootNode?.dataSourceId));
  const evidenceDigest = useEvidenceDigestModel(currentCase?.id, selectedFile, digestSource, activePartition?.index);

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
    treeLoading: tree.treeLoading,
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
    activateEntry,
    directoryLoadErrorText: pagination.rowsError || location.directoryError
      ? errorMessage(pagination.rowsError ?? location.directoryError) : undefined,
    retryDirectoryLoad: () => location.directoryError ? location.retryDirectory() : pagination.retryRows(),
    selectedFileId,
    viewerTab,
    fileSortKey: pagination.fileSortKey,
    fileSortDirection: pagination.fileSortDirection,
    handleSort,
    setSelectedDirectoryId,
    setSelectedFileId,
    setExpandedDirectoryIds: tree.setExpandedDirectoryIds,
    rowsPage: pagination.rowsPage,
    rowsContextKey: pagination.rowsContextKey,
    canGoToPreviousRows: pagination.canGoToPreviousRows,
    canGoToNextRows: pagination.canGoToNextRows,
    goToPreviousRows,
    goToNextRows,
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
    imageMetadata: preview.imageMetadata,
    imageMetadataLoading: preview.imageMetadataLoading,
    imageMetadataError: preview.imageMetadataError,
    onRetryImageMetadata: preview.onRetryImageMetadata,
    ntfsTechnical: ntfsTechnicalQuery.technical,
    ntfsTechnicalEligible: ntfsTechnicalQuery.eligible,
    ntfsTechnicalRuns: ntfsTechnicalQuery.runs,
    ntfsTechnicalLoading: ntfsTechnicalQuery.loading,
    ntfsTechnicalErrorText: ntfsTechnicalQuery.errorText,
    ntfsTechnicalUnsupported: ntfsTechnicalQuery.unsupported,
    retryNtfsTechnical: ntfsTechnicalQuery.retry,
    ntfsNavigation: ntfsTechnicalQuery,
    evidenceDigest,
    mediaUrl: preview.mediaUrl,
    documentPreview: preview.documentPreview,
    previewError: preview.previewError,
    onRetryPreview: preview.onRetryPreview,
    selectedFile,
    activePartition,
    bitLockerPartition: bitLockerTarget ? activePartition : undefined,
    bitLockerTargetKey: bitLockerTarget ? JSON.stringify([currentCase?.id, bitLockerTarget]) : undefined,
    bitLocker,
    activeDirectoryPath: location.directoryContext?.directory.path || pagination.activeDirectoryPath,
    currentDirectory: tree.currentDirectory,
    parentDirectory: tree.parentDirectory,
    goToParentDirectory: tree.goToParentDirectory,
    activeRootNode: tree.activeRootNode,
    executableCount: pagination.executableCount,
    fileExtraction,
    nestedEvidence,
    openNested: nestedEvidence.open,
    openNestedResult,
    onViewTimeline,
    dataSources,
  };
}
