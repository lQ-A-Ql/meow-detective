import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useFileChildrenPage, useFileTree as useFileTreeQuery } from '@/features/files/hooks';
import { useFileTreeKeyboard } from '@/hooks/use-file-tree-keyboard';
import { useResizablePanel } from '@/hooks/use-resizable-panel';
import { formatPartitionRootDisplayName } from '@/lib/partition-display';
import { mergeTreeNodePages, sameTreeNodeList } from '@/app/pages/file-tree-utils';
import type { DataSourcePartition, FileTreeNode } from '@/types/models';

const MAX_TREE_CACHE_SIZE = 100;

interface UseFileTreeOptions {
  showHidden: boolean;
  pageLimit: number;
  selectedDirectoryId?: string;
  setSelectedDirectoryId: (id?: string) => void;
  setSelectedFileId: (id?: string) => void;
  partitions: DataSourcePartition[];
}

export function useFileTree({
  showHidden,
  pageLimit,
  selectedDirectoryId,
  setSelectedDirectoryId,
  setSelectedFileId,
  partitions,
}: UseFileTreeOptions) {
  const [expandedDirectoryIds, setExpandedDirectoryIds] = useState<string[]>([]);
  const [treeChildren, setTreeChildren] = useState<Record<string, FileTreeNode[]>>({});
  const [treeChildOffsets, setTreeChildOffsets] = useState<Record<string, number>>({});
  const [filterQuery, setFilterQuery] = useState('');
  const treeContainerRef = useRef<HTMLDivElement>(null);
  const { width: treeWidth, isResizing, onResizeStart } = useResizablePanel({
    defaultWidth: 224,
    minWidth: 160,
    maxWidth: 400,
    storageKey: 'fileTreeWidth',
  });

  const { data: rootTree } = useFileTreeQuery(showHidden);
  const activeDirectoryId = selectedDirectoryId ?? rootTree?.[0]?.id;
  const activeDirectoryExpanded = Boolean(
    activeDirectoryId && expandedDirectoryIds.includes(activeDirectoryId)
  );
  const activeChildrenOffset = activeDirectoryId
    ? (treeChildOffsets[activeDirectoryId] ?? 0)
    : 0;
  const { data: activeChildrenPage } = useFileChildrenPage(
    activeDirectoryExpanded ? activeDirectoryId : undefined,
    activeChildrenOffset,
    pageLimit,
    showHidden
  );
  const activeChildren = activeChildrenPage?.children;

  useEffect(() => {
    setTreeChildren({});
    setTreeChildOffsets({});
  }, [showHidden]);

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

  const activeTreeChildrenLoaded = activeDirectoryId
    ? (treeChildren[activeDirectoryId]?.length ?? 0)
    : 0;
  const canLoadMoreTreeChildren = Boolean(
    activeDirectoryId &&
      activeChildrenPage?.truncated &&
      activeTreeChildrenLoaded < (activeChildrenPage?.totalCount ?? 0)
  );
  const loadMoreActiveTreeChildren = useCallback(() => {
    if (!activeDirectoryId) return;
    setTreeChildOffsets((current) => ({
      ...current,
      [activeDirectoryId]: (current[activeDirectoryId] ?? 0) + pageLimit,
    }));
  }, [activeDirectoryId, pageLimit]);

  return {
    rootTree,
    treeChildren,
    expandedDirectoryIds,
    setExpandedDirectoryIds,
    activeDirectoryId,
    activeChildrenPage,
    activeTreeChildrenLoaded,
    canLoadMoreTreeChildren,
    loadMoreActiveTreeChildren,
    toggleDirectory,
    handleNodeOpen,
    activeRootNode,
    displayNodeName,
    filterQuery,
    setFilterQuery,
    filteredTreeNodes,
    treeNodes,
    treeContainerRef,
    treeWidth,
    isResizing,
    onResizeStart,
    currentDirectory,
    parentDirectory,
    goToParentDirectory,
  };
}
