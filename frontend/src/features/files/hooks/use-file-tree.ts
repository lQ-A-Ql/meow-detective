import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useFileChildrenPage, useFileTree as useFileTreeQuery } from '@/features/files/hooks';
import { useFileTreeKeyboard } from '@/hooks/use-file-tree-keyboard';
import { useResizablePanel } from '@/hooks/use-resizable-panel';
import { formatPartitionRootDisplayName } from '@/lib/partition-display';
import { mergeTreeNodePages, sameTreeNodeList } from '@/app/pages/file-tree-utils';
import type {
  DataSourcePartition,
  DataSourceSummary,
  FileTreeNode,
} from '@/types/models';

const MAX_TREE_CACHE_SIZE = 100;
const DATA_SOURCE_NODE_PREFIX = 'data-source:';

function dataSourceNodeId(dsId: string): string {
  return `${DATA_SOURCE_NODE_PREFIX}${dsId}`;
}

export function isDataSourceTreeNodeId(nodeId: string | undefined): boolean {
  return Boolean(nodeId?.startsWith(DATA_SOURCE_NODE_PREFIX));
}

function rootsForDataSource(
  rootTree: FileTreeNode[] | undefined,
  dataSourceId: string,
  dataSources: DataSourceSummary[] | undefined,
) {
  const roots = rootTree ?? [];
  const hasSourceTaggedRoots = roots.some((node) => node.dataSourceId);
  if (!hasSourceTaggedRoots && dataSources?.length === 1) {
    return roots.map((node) => ({ ...node, depth: 1, dataSourceId }));
  }
  return roots
    .filter((node) => node.dataSourceId === dataSourceId)
    .map((node) => ({ ...node, depth: 1 }));
}

interface UseFileTreeOptions {
  showHidden: boolean;
  pageLimit: number;
  selectedDirectoryId?: string;
  setSelectedDirectoryId: (id?: string) => void;
  setSelectedFileId: (id?: string) => void;
  partitions: DataSourcePartition[];
  dataSources?: DataSourceSummary[];
}

export function useFileTree({
  showHidden,
  pageLimit,
  selectedDirectoryId,
  setSelectedDirectoryId,
  setSelectedFileId,
  partitions,
  dataSources,
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

  // Synthesize data-source parent nodes: wrap only each source's own root
  // directories/partition placeholders under that source. The backend tree nodes
  // carry dataSourceId, so we can avoid duplicating the whole case tree under
  // every data source.
  const wrappedRootTree = useMemo<FileTreeNode[]>(() => {
    if (!dataSources || dataSources.length === 0) return rootTree ?? [];
    return dataSources.map((ds) => ({
      id: dataSourceNodeId(ds.id),
      name: ds.name,
      depth: 0,
      hasChildren: rootsForDataSource(rootTree, ds.id, dataSources).length > 0,
      dataSourceId: ds.id,
      deleted: false,
      hidden: false,
      system: false,
    }));
  }, [dataSources, rootTree]);

  // Pre-populate tree children so data-source nodes show their own partition
  // roots without needing a backend fetch. Synthetic data-source IDs never exist
  // in the backend and must not be sent to file children/rows APIs.
  //
  // Always writes children directly (no sameTreeNodeList guard) because
  // React 18 batching can cause the effect to see stale current state
  // where the DS slot was never populated, while the wrappedRootTree memo
  // has already computed hasChildren=true from the fresh rootTree.
  useEffect(() => {
    if (!dataSources || dataSources.length === 0) return;
    setTreeChildren((current) => {
      const next = { ...current };
      for (const ds of dataSources) {
        const dsId = dataSourceNodeId(ds.id);
        next[dsId] = rootsForDataSource(rootTree, ds.id, dataSources);
      }
      return next;
    });
  }, [dataSources, rootTree]);

  const rootNodes = wrappedRootTree.length > 0 ? wrappedRootTree : (rootTree ?? []);

  // Auto-expand the first data source node so its partition children are visible
  // on initial load, matching the pre-wrap behavior where partition roots were at
  // depth 0 with no parent.
  useEffect(() => {
    if (wrappedRootTree.length > 0 && wrappedRootTree[0].hasChildren) {
      setExpandedDirectoryIds((current) =>
        current.includes(wrappedRootTree[0].id)
          ? current
          : [...current, wrappedRootTree[0].id],
      );
    }
  }, [wrappedRootTree]);

  const activeDirectoryId = selectedDirectoryId ?? rootNodes[0]?.id;
  const activeDirectoryIsDataSource = isDataSourceTreeNodeId(activeDirectoryId);
  const activeDirectoryExpanded = Boolean(
    activeDirectoryId && expandedDirectoryIds.includes(activeDirectoryId)
  );
  const activeChildrenOffset = activeDirectoryId
    ? (treeChildOffsets[activeDirectoryId] ?? 0)
    : 0;
  const backendChildrenParentId =
    activeDirectoryExpanded && !activeDirectoryIsDataSource ? activeDirectoryId : undefined;
  const { data: activeChildrenPage } = useFileChildrenPage(
    backendChildrenParentId,
    activeChildrenOffset,
    pageLimit,
    showHidden
  );
  const activeChildren = activeChildrenPage?.children;

  const showHiddenMountedRef = useRef(false);
  useEffect(() => {
    // Skip the reset on initial mount: firing here would race with (and wipe out)
    // the data-source children pre-population effect above, since both effects
    // run in declaration order during the same commit.
    if (!showHiddenMountedRef.current) {
      showHiddenMountedRef.current = true;
      return;
    }
    setTreeChildren({});
    setTreeChildOffsets({});
  }, [showHidden]);

  useEffect(() => {
    if (!activeDirectoryId || activeDirectoryIsDataSource || !activeChildren) return;
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
  }, [activeChildren, activeChildrenOffset, activeDirectoryId, activeDirectoryIsDataSource]);

  useEffect(() => {
    if (!selectedDirectoryId && rootNodes[0]?.id) {
      setSelectedDirectoryId(rootNodes[0].id);
      setExpandedDirectoryIds((current) =>
        current.includes(rootNodes[0].id)
          ? current
          : [...current, rootNodes[0].id]
      );
    }
  }, [rootNodes, selectedDirectoryId, setSelectedDirectoryId]);

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
    appendNodes(rootNodes);
    return visible;
  }, [expandedDirectoryIds, rootNodes, treeChildren]);

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
    if (!activeDirectoryId || !rootNodes.length) return rootNodes[0];
    for (const root of rootNodes) {
      if (root.id === activeDirectoryId) return root;
      const stack: FileTreeNode[] = [...(treeChildren[root.id] ?? [])];
      while (stack.length > 0) {
        const next: FileTreeNode = stack.pop()!;
        if (next.id === activeDirectoryId) return root;
        stack.push(...(treeChildren[next.id] ?? []));
      }
    }
    return rootNodes[0];
  }, [activeDirectoryId, rootNodes, treeChildren]);

  const displayNodeName = useCallback(
    (nodeName: string, depth = 0) => {
      if (depth === 0) return nodeName;
      if (depth === 1) return formatPartitionRootDisplayName(nodeName, partitions);
      return nodeName;
    },
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
      !activeDirectoryIsDataSource &&
      activeChildrenPage?.truncated &&
      activeTreeChildrenLoaded < (activeChildrenPage?.totalCount ?? 0)
  );
  const loadMoreActiveTreeChildren = useCallback(() => {
    if (!activeDirectoryId || activeDirectoryIsDataSource) return;
    setTreeChildOffsets((current) => ({
      ...current,
      [activeDirectoryId]: (current[activeDirectoryId] ?? 0) + pageLimit,
    }));
  }, [activeDirectoryId, activeDirectoryIsDataSource, pageLimit]);

  return {
    rootTree: rootNodes,
    treeChildren,
    expandedDirectoryIds,
    setExpandedDirectoryIds,
    activeDirectoryId,
    activeDirectoryIsDataSource,
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
