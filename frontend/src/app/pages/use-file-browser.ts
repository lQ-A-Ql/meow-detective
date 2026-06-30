import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useNavigate } from 'react-router';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import {
  useExtractFile, useFileChildrenPage, useFileHandle, useFileJumpContext,
  useFileRowsPage, useFileTree, useFileViewer, useImagePreview, useMediaUrl,
  useTextPreview,
} from '@/features/files/hooks';
import { useFileTreeKeyboard } from '@/hooks/use-file-tree-keyboard';
import { useResizablePanel } from '@/hooks/use-resizable-panel';
import { useSelectionStore } from '@/stores/selection-store';
import { useUiStore } from '@/stores/ui-store';
import { formatPartitionRootDisplayName } from '@/lib/partition-display';
import type { DataSourcePartition, FileTreeNode } from '@/types/models';
import type { FilePreviewKind } from './FilePreviewPanel';
import { mergeTreeNodePages, sameTreeNodeList } from './file-tree-utils';

const IMAGE_EXTENSIONS = new Set(['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp']);
const VIDEO_EXTENSIONS = new Set(['mp4', 'webm', 'avi', 'mkv']);
const AUDIO_EXTENSIONS = new Set(['mp3', 'wav', 'flac', 'aac', 'ogg']);

function getPreviewKindFromExtension(ext?: string): FilePreviewKind | undefined {
  const normalized = ext?.toLowerCase().replace(/^\./, '');
  if (!normalized) return undefined;
  if (IMAGE_EXTENSIONS.has(normalized)) return 'image';
  if (VIDEO_EXTENSIONS.has(normalized)) return 'video';
  if (AUDIO_EXTENSIONS.has(normalized)) return 'audio';
  return undefined;
}

function getPreviewKindFromMime(mime?: string): FilePreviewKind | undefined {
  const normalized = mime?.toLowerCase() ?? '';
  if (normalized.startsWith('image/')) return 'image';
  if (normalized.startsWith('video/')) return 'video';
  if (normalized.startsWith('audio/')) return 'audio';
  return undefined;
}

const FILE_BROWSER_PAGE_LIMIT = 500;
const MAX_TREE_CACHE_SIZE = 100;

export function useFileBrowser() {
  const navigate = useNavigate();
  const { data: currentCase } = useCurrentCase();
  const { data: dataSources } = useDataSources();
  const selectedDirectoryId = useSelectionStore((s) => s.selectedDirectoryId);
  const setSelectedDirectoryId = useSelectionStore((s) => s.setSelectedDirectoryId);
  const selectedFileId = useSelectionStore((s) => s.selectedFileId);
  const setSelectedFileId = useSelectionStore((s) => s.setSelectedFileId);
  const setSelectedTimelineId = useSelectionStore((s) => s.setSelectedTimelineId);
  const viewerTab = useUiStore((s) => s.viewerTab);
  const setViewerTab = useUiStore((s) => s.setViewerTab);
  const fileSortKey = useUiStore((s) => s.fileSortKey);
  const fileSortDirection = useUiStore((s) => s.fileSortDirection);
  const setFileSortKey = useUiStore((s) => s.setFileSortKey);
  const toggleFileSortDirection = useUiStore((s) => s.toggleFileSortDirection);
  const [expandedDirectoryIds, setExpandedDirectoryIds] = useState<string[]>([]);
  const [treeChildren, setTreeChildren] = useState<Record<string, FileTreeNode[]>>({});
  const [treeChildOffsets, setTreeChildOffsets] = useState<Record<string, number>>({});
  const [filterQuery, setFilterQuery] = useState('');
  const [showHidden, setShowHidden] = useState(false);
  const [rowsOffset, setRowsOffset] = useState(0);
  const treeContainerRef = useRef<HTMLDivElement>(null);
  const { width: treeWidth, isResizing, onResizeStart } = useResizablePanel({
    defaultWidth: 224,
    minWidth: 160,
    maxWidth: 400,
    storageKey: 'fileTreeWidth',
  });
  const { data: rootTree } = useFileTree(showHidden);
  const activeDirectoryId = selectedDirectoryId ?? rootTree?.[0]?.id;
  const activeDirectoryExpanded = Boolean(
    activeDirectoryId && expandedDirectoryIds.includes(activeDirectoryId)
  );
  const activeChildrenOffset = activeDirectoryId
    ? (treeChildOffsets[activeDirectoryId] ?? 0)
    : 0;
  const { data: rowsPage } = useFileRowsPage(
    activeDirectoryId,
    rowsOffset,
    FILE_BROWSER_PAGE_LIMIT,
    showHidden,
    fileSortKey,
    fileSortDirection
  );
  const { data: activeChildrenPage } = useFileChildrenPage(
    activeDirectoryExpanded ? activeDirectoryId : undefined,
    activeChildrenOffset,
    FILE_BROWSER_PAGE_LIMIT,
    showHidden
  );
  const rows = rowsPage?.rows;
  const {
    data: jumpContext,
    isLoading: jumpContextLoading,
    isFetching: jumpContextFetching,
  } = useFileJumpContext(
    selectedFileId,
    showHidden,
    FILE_BROWSER_PAGE_LIMIT,
    fileSortKey,
    fileSortDirection
  );
  const activeChildren = activeChildrenPage?.children;
  const partitions = useMemo<DataSourcePartition[]>(
    () => dataSources?.flatMap((source) => source.partitions ?? []) ?? [],
    [dataSources]
  );
  useEffect(() => {
    setTreeChildren({});
    setTreeChildOffsets({});
    setRowsOffset(0);
  }, [showHidden]);
  useEffect(() => {
    setRowsOffset(0);
  }, [activeDirectoryId, fileSortKey, fileSortDirection]);
  useEffect(() => {
    if (!activeDirectoryId || !activeChildren) return;
    const pageOffset = activeChildrenOffset;
    setTreeChildren((current) => {
      const keys = Object.keys(current);
      const previousChildren = current[activeDirectoryId] ?? [];
      const nextChildren = pageOffset > 0
        ? mergeTreeNodePages(previousChildren, activeChildren)
        : activeChildren;
      if (sameTreeNodeList(previousChildren, nextChildren)) return current;
      if (!current[activeDirectoryId] && keys.length >= MAX_TREE_CACHE_SIZE) {
        const { [keys[0]]: _, ...rest } = current;
        return { ...rest, [activeDirectoryId]: nextChildren };
      }
      return { ...current, [activeDirectoryId]: nextChildren };
    });
  }, [activeChildren, activeChildrenOffset, activeDirectoryId]);
  useEffect(() => {
    if (!selectedDirectoryId && rootTree?.[0]?.id) {
      setSelectedDirectoryId(rootTree[0].id);
      setExpandedDirectoryIds((current) =>
        current.includes(rootTree[0].id)
          ? current
          : [...current, rootTree[0].id]
      );
    }
  }, [rootTree, selectedDirectoryId, setSelectedDirectoryId]);
  useEffect(() => {
    const visibleIds = new Set<string>();
    const collect = (nodes: FileTreeNode[]) => {
      for (const node of nodes) {
        visibleIds.add(node.id);
        const children = treeChildren[node.id];
        if (children?.length) collect(children);
      }
    };
    collect(rootTree ?? []);
    if (jumpContext) {
      visibleIds.add(jumpContext.directory.id);
      for (const id of jumpContext.ancestorDirectoryIds) visibleIds.add(id);
    }
    if (selectedDirectoryId && !visibleIds.has(selectedDirectoryId)) {
      setSelectedDirectoryId(rootTree?.[0]?.id);
      setSelectedFileId(undefined);
    }
  }, [
    jumpContext,
    rootTree,
    selectedDirectoryId,
    setSelectedDirectoryId,
    setSelectedFileId,
    treeChildren,
  ]);
  useEffect(() => {
    if (
      selectedFileId &&
      (!rows || !rows.some((row) => row.id === selectedFileId))
    ) {
      if (!jumpContext && !jumpContextLoading && !jumpContextFetching) {
        setSelectedFileId(undefined);
      }
    }
  }, [
    jumpContext,
    jumpContextFetching,
    jumpContextLoading,
    rows,
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
    setExpandedDirectoryIds((current) => {
      const next = new Set(current);
      for (const id of jumpContext.ancestorDirectoryIds) next.add(id);
      return next.size === current.length ? current : Array.from(next);
    });
    if (rowsOffset !== jumpContext.rowOffset) {
      setRowsOffset(jumpContext.rowOffset);
    }
  }, [
    jumpContext,
    rowsOffset,
    selectedDirectoryId,
    selectedFileId,
    setSelectedDirectoryId,
    showHidden,
  ]);
  const selectedFile = rows?.find((row) => row.id === selectedFileId);
  const selectedFilePreviewKind = useMemo(() => {
    const ext = selectedFile?.ext ?? selectedFile?.name.split('.').pop();
    return getPreviewKindFromExtension(ext);
  }, [selectedFile?.ext, selectedFile?.name]);
  const needsPreviewHandle =
    viewerTab === 'preview' &&
    Boolean(selectedFile?.id) &&
    selectedFilePreviewKind === undefined;
  const needsMetadataHandle =
    viewerTab === 'metadata' && Boolean(selectedFile?.id);
  const { data: fileHandle } = useFileHandle(
    selectedFile?.id,
    needsPreviewHandle || needsMetadataHandle
  );
  const previewKind =
    viewerTab === 'preview'
      ? selectedFilePreviewKind ?? getPreviewKindFromMime(fileHandle?.mime)
      : undefined;
  const hexPreviewEnabled = viewerTab === 'hex' && Boolean(selectedFile?.id);
  const textPreviewEnabled = viewerTab === 'text' && Boolean(selectedFile?.id);
  const imagePreviewEnabled =
    viewerTab === 'preview' && previewKind === 'image' && Boolean(selectedFile?.id);
  const mediaPreviewEnabled =
    viewerTab === 'preview' &&
    (previewKind === 'video' || previewKind === 'audio') &&
    Boolean(selectedFile?.id);
  const {
    data: viewer,
    setJumpOffsetInput,
    jumpToOffset,
    loadNextRange,
    loadPreviousRange,
  } = useFileViewer(selectedFile?.id, hexPreviewEnabled);
  const { data: textPreview } = useTextPreview(selectedFile?.id, textPreviewEnabled);
  const { data: imagePreview } = useImagePreview(selectedFile?.id, imagePreviewEnabled);
  const { data: mediaUrl } = useMediaUrl(selectedFile?.id, mediaPreviewEnabled);
  const extractFile = useExtractFile();
  const flatTree = useMemo(() => {
    const visible: FileTreeNode[] = [];
    const appendNodes = (nodes: FileTreeNode[]) => {
      for (const node of nodes) {
        visible.push(node);
        if (expandedDirectoryIds.includes(node.id)) {
          appendNodes(treeChildren[node.id] ?? []);
        }
      }
    };
    appendNodes(rootTree ?? []);
    return visible;
  }, [expandedDirectoryIds, rootTree, treeChildren]);
  const currentDirectory = flatTree.find((node) => node.id === activeDirectoryId);
  const parentDirectory = useMemo(() => {
    if (!currentDirectory) return undefined;
    const idx = flatTree.findIndex((node) => node.id === currentDirectory.id);
    if (idx <= 0) return undefined;
    return flatTree
      .slice(0, idx)
      .reverse()
      .find((node) => node.depth === currentDirectory.depth - 1);
  }, [currentDirectory, flatTree]);
  const goToParentDirectory = useCallback(() => {
    if (!parentDirectory) return;
    setSelectedDirectoryId(parentDirectory.id);
    setSelectedFileId(undefined);
  }, [parentDirectory, setSelectedDirectoryId, setSelectedFileId]);
  const activeRootNode = useMemo(() => {
    if (!activeDirectoryId || !rootTree?.length) return rootTree?.[0];
    for (const root of rootTree) {
      if (root.id === activeDirectoryId) return root;
      const stack: FileTreeNode[] = [...(treeChildren[root.id] ?? [])];
      while (stack.length > 0) {
        const next: FileTreeNode = stack.pop()!;
        if (next.id === activeDirectoryId) return root;
        stack.push(...(treeChildren[next.id] ?? []));
      }
    }
    return rootTree[0];
  }, [activeDirectoryId, rootTree, treeChildren]);
  const displayNodeName = useCallback(
    (nodeName: string, depth = 0) =>
      depth !== 0
        ? nodeName
        : formatPartitionRootDisplayName(nodeName, partitions),
    [partitions]
  );
  const executableCount = useMemo(
    () =>
      rows?.filter((row) =>
        ['exe', 'dll'].includes(
          (row.ext ?? row.name.split('.').pop() ?? '').toLowerCase()
        )
      ).length ?? 0,
    [rows]
  );
  const sortedRows = rows ?? [];
  const treeNodes = useMemo(
    () =>
      flatTree.map((node) => ({
        ...node,
        active: node.id === activeDirectoryId,
        expanded: expandedDirectoryIds.includes(node.id),
      })),
    [activeDirectoryId, expandedDirectoryIds, flatTree]
  );
  const filteredTreeNodes = useMemo(() => {
    if (!filterQuery.trim()) return treeNodes;
    const query = filterQuery.toLowerCase();
    return treeNodes.filter((node) =>
      node.name.toLowerCase().includes(query)
    );
  }, [treeNodes, filterQuery]);
  const toggleDirectory = useCallback(
    (node: FileTreeNode) => {
      setSelectedDirectoryId(node.id);
      setSelectedFileId(undefined);
      setExpandedDirectoryIds((current) =>
        current.includes(node.id)
          ? current.filter((id) => id !== node.id)
          : [...current, node.id]
      );
    },
    [setSelectedDirectoryId, setSelectedFileId]
  );
  const handleNodeOpen = useCallback(
    (nodeId: string) => {
      const node = treeNodes.find((n) => n.id === nodeId);
      if (node?.hasChildren) toggleDirectory(node);
      else if (node) setSelectedFileId(node.id);
    },
    [treeNodes, toggleDirectory, setSelectedFileId]
  );
  useFileTreeKeyboard({
    nodes: filteredTreeNodes,
    activeNodeId: activeDirectoryId,
    onNodeSelect: setSelectedDirectoryId,
    onNodeToggle: (id) =>
      setExpandedDirectoryIds((current) =>
        current.includes(id)
          ? current.filter((i) => i !== id)
          : [...current, id]
      ),
    onNodeOpen: handleNodeOpen,
    scrollContainerRef: treeContainerRef,
  });
  const activeDirectoryPath = rows?.find(
    (row) => row.id === activeDirectoryId
  )?.path;
  const handleSort = useCallback(
    (key: string) => {
      if (key === fileSortKey) {
        toggleFileSortDirection();
      } else {
        setFileSortKey(key as 'name' | 'size' | 'modifiedAt' | 'ext');
      }
    },
    [fileSortKey, setFileSortKey, toggleFileSortDirection]
  );
  const activeTreeChildrenLoaded = activeDirectoryId
    ? (treeChildren[activeDirectoryId]?.length ?? 0)
    : 0;
  const canLoadMoreTreeChildren = Boolean(
    activeDirectoryId &&
      activeChildrenPage?.truncated &&
      activeTreeChildrenLoaded < (activeChildrenPage?.totalCount ?? 0)
  );
  const canGoToPreviousRows = rowsOffset > 0;
  const canGoToNextRows = Boolean(
    rowsPage && rowsPage.offset + rowsPage.limit < rowsPage.totalCount
  );
  const loadMoreActiveTreeChildren = useCallback(() => {
    if (!activeDirectoryId) return;
    setTreeChildOffsets((current) => ({
      ...current,
      [activeDirectoryId]:
        (current[activeDirectoryId] ?? 0) + FILE_BROWSER_PAGE_LIMIT,
    }));
  }, [activeDirectoryId]);
  const goToPreviousRows = useCallback(
    () => setRowsOffset((current) => Math.max(0, current - FILE_BROWSER_PAGE_LIMIT)),
    []
  );
  const goToNextRows = useCallback(() => {
    if (!rowsPage) return;
    setRowsOffset((current) =>
      current + rowsPage.limit < rowsPage.totalCount
        ? current + FILE_BROWSER_PAGE_LIMIT
        : current
    );
  }, [rowsPage]);
  const onViewTimeline = useCallback(() => {
    if (selectedFile) {
      setSelectedTimelineId(selectedFile.id);
      navigate('/timeline');
    }
  }, [selectedFile, setSelectedTimelineId, navigate]);
  return {
    currentCase,
    treeWidth,
    isResizing,
    onResizeStart,
    filteredTreeNodes,
    activeChildrenPage,
    activeTreeChildrenLoaded,
    canLoadMoreTreeChildren,
    loadMoreActiveTreeChildren,
    toggleDirectory,
    displayNodeName,
    filterQuery,
    setFilterQuery,
    treeContainerRef,
    FILE_BROWSER_PAGE_LIMIT,
    showHidden,
    setShowHidden,
    treeNodes,
    sortedRows,
    selectedFileId,
    viewerTab,
    fileSortKey,
    fileSortDirection,
    handleSort,
    setSelectedDirectoryId,
    setSelectedFileId,
    setExpandedDirectoryIds,
    rowsPage,
    canGoToPreviousRows,
    canGoToNextRows,
    goToPreviousRows,
    goToNextRows,
    hexPreviewEnabled,
    viewer,
    fileHandle,
    previewKind,
    setViewerTab,
    setJumpOffsetInput,
    jumpToOffset,
    loadNextRange,
    loadPreviousRange,
    textPreview,
    imagePreview,
    mediaUrl,
    selectedFile,
    activeDirectoryPath,
    currentDirectory,
    parentDirectory,
    goToParentDirectory,
    activeRootNode,
    executableCount,
    extractFile,
    onViewTimeline,
  };
}
