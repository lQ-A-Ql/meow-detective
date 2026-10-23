import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import {
  useFileChildrenBranches,
  useFileTree as useFileTreeQuery,
  type FileChildrenBranchRequest,
} from '@/features/files/hooks';
import { useFileTreeKeyboard } from '@/hooks/use-file-tree-keyboard';
import { useResizablePanel } from '@/hooks/use-resizable-panel';
import { formatPartitionRootDisplayName } from '@/lib/partition-display';
import {
  findTreeNode,
  findTreeRoot,
  findEvictableTreeCacheKey,
  flattenExpandedTree,
  mergeTreeNodePages,
  rebaseTreeNodeDepths,
  sameTreeNodeList,
} from '@/features/files/file-tree-utils';
import {
  DEFAULT_FILE_TREE_SCOPE,
  readFileTreeExpansion,
  writeFileTreeExpansion,
} from '@/features/files/file-tree-expansion-store';
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

function normalizeChildrenForParent(
  children: FileTreeNode[],
  parent: FileTreeNode | undefined,
) {
  const normalized = rebaseTreeNodeDepths(children, parent?.depth ?? -1);
  if (!parent) return normalized;
  return normalized.filter((child) => child.id !== parent.id);
}

interface UseFileTreeOptions {
  showHidden: boolean;
  pageLimit: number;
  selectedDirectoryId?: string;
  setSelectedDirectoryId: (id?: string) => void;
  setSelectedFileId: (id?: string) => void;
  partitions: DataSourcePartition[];
  dataSources?: DataSourceSummary[];
  /** Case that owns the expansion state, so switching cases starts from a clean tree. */
  caseId?: string;
  /**
   * Directory ids that must be materialized even though they are not reachable from
   * the current tree yet (search jump / nested-evidence open). Their ancestor levels
   * are fetched in parallel instead of one level per commit.
   */
  revealDirectoryIds?: readonly string[];
  /** Backend-proven path nodes for a jump; avoids guessing ancestor pages. */
  jumpPathNodes?: readonly FileTreeNode[];
}

export function useFileTree({
  showHidden,
  pageLimit,
  selectedDirectoryId,
  setSelectedDirectoryId,
  setSelectedFileId,
  partitions,
  dataSources,
  caseId,
  revealDirectoryIds,
  jumpPathNodes,
}: UseFileTreeOptions) {
  const expansionScope = caseId ?? DEFAULT_FILE_TREE_SCOPE;
  const [expandedDirectoryIds, setExpandedDirectoryIdsState] = useState<string[]>(
    () => readFileTreeExpansion(expansionScope),
  );
  const [treeChildren, setTreeChildren] = useState<Record<string, FileTreeNode[]>>({});
  const [treeChildOffsets, setTreeChildOffsets] = useState<Record<string, number>>({});
  const [filterQuery, setFilterQuery] = useState('');
  const treeContainerRef = useRef<HTMLDivElement>(null);
  const expansionScopeRef = useRef(expansionScope);
  const { width: treeWidth, isResizing, onResizeStart } = useResizablePanel({
    defaultWidth: 224,
    minWidth: 160,
    maxWidth: 400,
    storageKey: 'fileTreeWidth',
  });

  // A case switch must not inherit the previous case's expansion or cached children.
  if (expansionScopeRef.current !== expansionScope) {
    expansionScopeRef.current = expansionScope;
    setExpandedDirectoryIdsState(readFileTreeExpansion(expansionScope));
    setTreeChildren({});
    setTreeChildOffsets({});
  }

  const setExpandedDirectoryIds = useCallback(
    (updater: (current: string[]) => string[]) => {
      setExpandedDirectoryIdsState((current) => {
        const next = updater(current);
        if (next !== current) writeFileTreeExpansion(expansionScope, next);
        return next;
      });
    },
    [expansionScope],
  );

  const { data: rootTree, isFetching: rootTreeFetching } = useFileTreeQuery(showHidden, caseId);

  const showHiddenMountedRef = useRef(false);
  useEffect(() => {
    // Skip the reset on initial mount: firing here would race with (and wipe out)
    // the data-source children pre-population effect below, since both effects
    // run in declaration order during the same commit.
    //
    // This effect is declared BEFORE the pre-population and merge effects so
    // that on a visibility toggle the wipe lands first and those effects can
    // immediately repopulate from the current (possibly referentially
    // unchanged) query data in the same commit.
    if (!showHiddenMountedRef.current) {
      showHiddenMountedRef.current = true;
      return;
    }
    setTreeChildren({});
    setTreeChildOffsets({});
  }, [showHidden]);

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
  //
  // `showHidden` is a dependency on purpose: the reset effect below wipes
  // the children map on every visibility toggle, and react-query's
  // placeholderData + structural sharing can keep `rootTree` referentially
  // identical across a quick A→B→A toggle. Without re-running here, the DS
  // children would stay wiped and the tree could not expand until the next
  // fetch produced a new rootTree reference.
  useEffect(() => {
    if (!dataSources || dataSources.length === 0) return;
    setTreeChildren((current) => {
      const next = { ...current };
      for (const ds of dataSources) {
        const dsId = dataSourceNodeId(ds.id);
        const parent = wrappedRootTree.find((node) => node.id === dsId);
        next[dsId] = normalizeChildrenForParent(
          rootsForDataSource(rootTree, ds.id, dataSources),
          parent,
        );
      }
      return next;
    });
  }, [dataSources, rootTree, wrappedRootTree, showHidden]);

  useEffect(() => {
    if (!jumpPathNodes?.length) return;
    setTreeChildren((current) => {
      const next = { ...current };
      const first = jumpPathNodes[0];
      const dataSourceId = first.dataSourceId;
      if (dataSourceId && dataSources?.some((source) => source.id === dataSourceId)) {
        const sourceNodeId = dataSourceNodeId(dataSourceId);
        const parent = wrappedRootTree.find((node) => node.id === sourceNodeId);
        if (parent) {
          next[sourceNodeId] = normalizeChildrenForParent([first], parent);
        }
      }
      for (let index = 0; index < jumpPathNodes.length - 1; index += 1) {
        const parent = jumpPathNodes[index];
        const child = jumpPathNodes[index + 1];
        next[parent.id] = normalizeChildrenForParent([child], parent);
      }
      return next;
    });
  }, [dataSources, jumpPathNodes, showHidden, wrappedRootTree]);

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
  }, [setExpandedDirectoryIds, wrappedRootTree]);

  const activeDirectoryId = selectedDirectoryId ?? rootNodes[0]?.id;
  const activeDirectoryIsDataSource = isDataSourceTreeNodeId(activeDirectoryId);
  const expandedIdSet = useMemo(() => new Set(expandedDirectoryIds), [expandedDirectoryIds]);

  const flatTree = useMemo(
    () => flattenExpandedTree(rootNodes, treeChildren, expandedIdSet),
    [expandedIdSet, rootNodes, treeChildren],
  );

  // Every expanded branch that is already visible gets its children, not just the
  // active directory. The walk loads one level per commit, so restoring a large
  // remembered expansion set cannot fan out into a query per remembered branch.
  const branchRequests = useMemo<FileChildrenBranchRequest[]>(() => {
    const parentIds = new Set<string>();
    for (const node of flatTree) {
      if (expandedIdSet.has(node.id) && !isDataSourceTreeNodeId(node.id)) {
        parentIds.add(node.id);
      }
    }
    if (
      activeDirectoryId
      && !activeDirectoryIsDataSource
      && expandedIdSet.has(activeDirectoryId)
    ) {
      parentIds.add(activeDirectoryId);
    }
    // A programmatic reveal knows the ancestor ids before they are reachable, so
    // those levels are fetched together instead of stalling the tree walk.
    for (const id of revealDirectoryIds ?? []) {
      if (id && !isDataSourceTreeNodeId(id)) parentIds.add(id);
    }
    return Array.from(parentIds, (parentId) => ({
      parentId,
      offset: treeChildOffsets[parentId] ?? 0,
    }));
  }, [
    activeDirectoryId,
    activeDirectoryIsDataSource,
    expandedIdSet,
    flatTree,
    revealDirectoryIds,
    treeChildOffsets,
  ]);

  const branchQueries = useFileChildrenBranches(branchRequests, pageLimit, showHidden, caseId);

  const jumpPathChildren = useMemo(() => {
    const result = new Map<string, FileTreeNode>();
    if (!jumpPathNodes?.length) return result;
    for (let index = 1; index < jumpPathNodes.length; index += 1) {
      result.set(jumpPathNodes[index - 1].id, jumpPathNodes[index]);
    }
    return result;
  }, [jumpPathNodes]);

  useEffect(() => {
    if (branchRequests.length === 0) return;
    setTreeChildren((current) => {
      let next = current;
      for (let index = 0; index < branchRequests.length; index += 1) {
        const request = branchRequests[index];
        const page = branchQueries[index]?.data;
        if (!page) continue;
        const parent =
          findTreeNode(request.parentId, rootNodes, next)
          ?? findTreeNode(request.parentId, rootNodes, current);
        const normalized = normalizeChildrenForParent(page.children, parent);
        const jumpChild = jumpPathChildren.get(request.parentId);
        const normalizedWithJumpChild = jumpChild && !normalized.some((node) => node.id === jumpChild.id)
          ? [...normalized, jumpChild]
          : normalized;
        const previous = next[request.parentId] ?? current[request.parentId] ?? [];
        const merged = request.offset > 0 || jumpChild
          ? mergeTreeNodePages(previous, normalizedWithJumpChild)
          : normalizedWithJumpChild;
        if (sameTreeNodeList(previous, merged)) continue;
        if (!current[request.parentId] && Object.keys(current).length >= MAX_TREE_CACHE_SIZE) {
          // Keep every expanded, visible branch resident. Evicting an ancestor
          // would make its whole subtree disappear despite its expanded state.
          const pinnedIds = new Set(Object.keys(current).filter(isDataSourceTreeNodeId));
          const evictKey = findEvictableTreeCacheKey(
            rootNodes,
            current,
            expandedIdSet,
            pinnedIds,
          );
          if (evictKey) {
            const { [evictKey]: _dropped, ...rest } = next;
            next = rest;
          }
        }
        if (next === current) next = { ...current };
        next[request.parentId] = merged;
      }
      return next;
    });
    // `showHidden` is included so cached branches are re-merged after the
    // visibility-reset wipe even when the query data reference is unchanged.
  }, [
    branchQueries,
    branchRequests,
    expandedIdSet,
    jumpPathChildren,
    rootNodes,
    showHidden,
    treeChildren,
  ]);

  useEffect(() => {
    if (!selectedDirectoryId && rootNodes[0]?.id) {
      setSelectedDirectoryId(rootNodes[0].id);
      setExpandedDirectoryIds((current) =>
        current.includes(rootNodes[0].id)
          ? current
          : [...current, rootNodes[0].id]
      );
    }
  }, [rootNodes, selectedDirectoryId, setExpandedDirectoryIds, setSelectedDirectoryId]);

  const activeBranchIndex = useMemo(
    () => branchRequests.findIndex((request) => request.parentId === activeDirectoryId),
    [activeDirectoryId, branchRequests],
  );
  const activeChildrenPage = activeBranchIndex >= 0
    ? branchQueries[activeBranchIndex]?.data
    : undefined;

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
    return findTreeRoot(activeDirectoryId, rootNodes, treeChildren) ?? rootNodes[0];
  }, [activeDirectoryId, rootNodes, treeChildren]);

  const displayNodeName = useCallback(
    (nodeName: string, depth = 0, dataSourceId?: string) => {
      if (depth === 0) return nodeName;
      if (depth === 1) {
        const sourcePartitions = dataSources?.find((source) => source.id === dataSourceId)?.partitions;
        return formatPartitionRootDisplayName(nodeName, sourcePartitions ?? partitions);
      }
      return nodeName;
    },
    [dataSources, partitions]
  );

  // Keep node object references stable: selection/expansion flags are derived
  // inside the memoized row components (FileTreePanel) instead of spreading
  // every node into a fresh object on each state change.
  const filteredTreeNodes = useMemo(() => {
    if (!filterQuery.trim()) return flatTree;
    const query = filterQuery.toLowerCase();
    return flatTree.filter((node) =>
      node.name.toLowerCase().includes(query)
    );
  }, [flatTree, filterQuery]);

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
    [setExpandedDirectoryIds, setSelectedDirectoryId, setSelectedFileId]
  );
  const handleNodeOpen = useCallback(
    (nodeId: string) => {
      const node = flatTree.find((n) => n.id === nodeId);
      if (node?.hasChildren) toggleDirectory(node);
      else if (node) setSelectedFileId(node.id);
    },
    [flatTree, toggleDirectory, setSelectedFileId]
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

  // True once the root tree and every requested branch have settled. Callers use it
  // to decide whether a selected directory is genuinely unreachable (and should be
  // reset) or simply still being materialized.
  //
  // A branch that is reachable and expanded but has no children entry yet is still
  // materializing even when its query already resolved - the merge lands in the next
  // commit - so it must keep the tree unsettled until the walk reaches the deepest
  // expanded level.
  const branchesMaterialized = branchRequests.every(
    (request) => treeChildren[request.parentId] !== undefined,
  );
  const treeSettled = rootTree !== undefined
    && !rootTreeFetching
    && !branchQueries.some((query) => query.isFetching)
    && branchesMaterialized;

  return {
    rootTree: rootNodes,
    treeLoading: rootTreeFetching && rootTree === undefined,
    treeSettled,
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
    treeNodes: flatTree,
    expandedIdSet,
    treeContainerRef,
    treeWidth,
    isResizing,
    onResizeStart,
    currentDirectory,
    parentDirectory,
    goToParentDirectory,
  };
}
