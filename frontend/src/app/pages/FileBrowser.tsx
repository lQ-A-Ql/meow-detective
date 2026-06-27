import { useEffect, useMemo, useState, useCallback, useRef } from 'react';
import { useNavigate } from 'react-router';
import { ChevronRight, HardDrive } from 'lucide-react';
import { PageSubbar } from '@/components/layout/PageSubbar';
import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import { useResizablePanel } from '@/hooks/use-resizable-panel';
import { FileVisibilityToggle } from '@/components/files/FileVisibilityToggle';
import { useFileTreeKeyboard } from '@/hooks/use-file-tree-keyboard';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import {
  useExtractFile,
  useFileChildrenPage,
  useFileJumpContext,
  useFileRowsPage,
  useFileTree,
  useFileViewer,
  useTextPreview,
  useImagePreview,
  useMediaUrl,
} from '@/features/files/hooks';
import { useSelectionStore } from '@/stores/selection-store';
import { useUiStore } from '@/stores/ui-store';
import { formatPartitionRootDisplayName } from '@/lib/partition-display';
import {
  DataSourcePartition,
  FileTreeNode,
} from '@/types/models';
import { FileTreePanel } from './FileTreePanel';
import { FileListPanel } from './FileListPanel';
import { FilePreviewPanel } from './FilePreviewPanel';
import { sameTreeNodeList, mergeTreeNodePages } from './file-tree-utils';

export function FileBrowser() {
  const navigate = useNavigate();
  const { data: currentCase } = useCurrentCase();
  const { data: dataSources } = useDataSources();
  const selectedDirectoryId = useSelectionStore(
    (state) => state.selectedDirectoryId
  );
  const setSelectedDirectoryId = useSelectionStore(
    (state) => state.setSelectedDirectoryId
  );
  const selectedFileId = useSelectionStore((state) => state.selectedFileId);
  const setSelectedFileId = useSelectionStore(
    (state) => state.setSelectedFileId
  );
  const setSelectedTimelineId = useSelectionStore(
    (state) => state.setSelectedTimelineId
  );
  const viewerTab = useUiStore((state) => state.viewerTab);
  const setViewerTab = useUiStore((state) => state.setViewerTab);
  const fileSortKey = useUiStore((state) => state.fileSortKey);
  const fileSortDirection = useUiStore((state) => state.fileSortDirection);
  const setFileSortKey = useUiStore((state) => state.setFileSortKey);
  const toggleFileSortDirection = useUiStore(
    (state) => state.toggleFileSortDirection
  );
  const [expandedDirectoryIds, setExpandedDirectoryIds] = useState<string[]>(
    []
  );
  const [treeChildren, setTreeChildren] = useState<
    Record<string, FileTreeNode[]>
  >({});
  const [treeChildOffsets, setTreeChildOffsets] = useState<
    Record<string, number>
  >({});
  const [filterQuery, setFilterQuery] = useState('');
  const [showHidden, setShowHidden] = useState(false);
  const [rowsOffset, setRowsOffset] = useState(0);
  const treeContainerRef = useRef<HTMLDivElement>(null);
  const FILE_BROWSER_PAGE_LIMIT = 500;

  // 可调整宽度的面板
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
    fileSortDirection,
  );
  const activeChildren = activeChildrenPage?.children;
  const partitions = useMemo<DataSourcePartition[]>(
    () => dataSources?.flatMap((source) => source.partitions ?? []) ?? [],
    [dataSources],
  );

  // 缓存限制常量
  const MAX_TREE_CACHE_SIZE = 100;

  useEffect(() => {
    setTreeChildren({});
    setTreeChildOffsets({});
    setRowsOffset(0);
  }, [showHidden]);

  // 目录切换或排序变化时回到第一页
  useEffect(() => {
    setRowsOffset(0);
  }, [activeDirectoryId, fileSortKey, fileSortDirection]);

  useEffect(() => {
    if (!activeDirectoryId || !activeChildren) {
      return;
    }
    const pageOffset = activeChildrenOffset;
    setTreeChildren((current) => {
      const keys = Object.keys(current);
      const previousChildren = current[activeDirectoryId] ?? [];
      const nextChildren =
        pageOffset > 0
          ? mergeTreeNodePages(previousChildren, activeChildren)
          : activeChildren;

      if (sameTreeNodeList(previousChildren, nextChildren)) {
        return current;
      }

      // 如果缓存超过限制，删除最早的条目
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
        if (children?.length) {
          collect(children);
        }
      }
    };
    collect(rootTree ?? []);
    if (jumpContext) {
      visibleIds.add(jumpContext.directory.id);
      for (const id of jumpContext.ancestorDirectoryIds) {
        visibleIds.add(id);
      }
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
    if (!jumpContext || !selectedFileId || jumpContext.target.id !== selectedFileId) {
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
      for (const id of jumpContext.ancestorDirectoryIds) {
        next.add(id);
      }
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
  const {
    data: viewer,
    setJumpOffsetInput,
    jumpToOffset,
    loadNextRange,
    loadPreviousRange,
  } = useFileViewer(selectedFile?.id);
  const { data: textPreview } = useTextPreview(selectedFile?.id);
  const { data: imagePreview } = useImagePreview(selectedFile?.id);
  const { data: mediaUrl } = useMediaUrl(selectedFile?.id);
  const extractFile = useExtractFile();

  const flatTree = useMemo(() => {
    const visible: FileTreeNode[] = [];
    const roots = rootTree ?? [];

    const appendNodes = (nodes: FileTreeNode[]) => {
      for (const node of nodes) {
        visible.push(node);
        if (expandedDirectoryIds.includes(node.id)) {
          appendNodes(treeChildren[node.id] ?? []);
        }
      }
    };

    appendNodes(roots);
    return visible;
  }, [expandedDirectoryIds, rootTree, treeChildren]);

  const currentDirectory = flatTree.find(
    (node) => node.id === activeDirectoryId
  );

  const activeRootNode = useMemo(() => {
    if (!activeDirectoryId || !rootTree?.length) {
      return rootTree?.[0];
    }

    for (const root of rootTree) {
      if (root.id === activeDirectoryId) {
        return root;
      }

      const stack: FileTreeNode[] = [...(treeChildren[root.id] ?? [])];
      while (stack.length > 0) {
        const next: FileTreeNode = stack.pop()!;
        if (next.id === activeDirectoryId) {
          return root;
        }
        stack.push(...(treeChildren[next.id] ?? []));
      }
    }

    return rootTree[0];
  }, [activeDirectoryId, rootTree, treeChildren]);

  function displayNodeName(nodeName: string, depth = 0) {
    if (depth !== 0) {
      return nodeName;
    }
    return formatPartitionRootDisplayName(nodeName, partitions);
  }

  const executableCount =
    rows?.filter((row) =>
      ['exe', 'dll'].includes(
        (row.ext ?? row.name.split('.').pop() ?? '').toLowerCase()
      )
    ).length ?? 0;

  // 后端已在完整可见集合上完成「目录优先 + 状态后置 + 自然排序」并分页，
  // 前端直接展示返回顺序，避免页内二次排序造成的假象。
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

  // 过滤后的树节点
  const filteredTreeNodes = useMemo(() => {
    if (!filterQuery.trim()) return treeNodes;
    const query = filterQuery.toLowerCase();
    return treeNodes.filter((node) =>
      node.name.toLowerCase().includes(query)
    );
  }, [treeNodes, filterQuery]);

  // 键盘导航
  const handleNodeOpen = useCallback(
    (nodeId: string) => {
      const node = treeNodes.find((n) => n.id === nodeId);
      if (node?.hasChildren) {
        toggleDirectory(node);
      } else if (node) {
        setSelectedFileId(node.id);
      }
    },
    [treeNodes]
  );

  useFileTreeKeyboard({
    nodes: filteredTreeNodes,
    activeNodeId: activeDirectoryId,
    onNodeSelect: setSelectedDirectoryId,
    onNodeToggle: (id) => {
      setExpandedDirectoryIds((current) =>
        current.includes(id)
          ? current.filter((i) => i !== id)
          : [...current, id]
      );
    },
    onNodeOpen: handleNodeOpen,
    scrollContainerRef: treeContainerRef,
  });

  const activeDirectoryPath = rows?.find(
    (row) => row.id === activeDirectoryId
  )?.path;

  // 处理排序
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

  function toggleDirectory(node: FileTreeNode) {
    setSelectedDirectoryId(node.id);
    setSelectedFileId(undefined);
    setExpandedDirectoryIds((current) =>
      current.includes(node.id)
        ? current.filter((id) => id !== node.id)
        : [...current, node.id]
    );
  }

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
    if (!activeDirectoryId) {
      return;
    }
    setTreeChildOffsets((current) => ({
      ...current,
      [activeDirectoryId]: (current[activeDirectoryId] ?? 0) + FILE_BROWSER_PAGE_LIMIT,
    }));
  }, [activeDirectoryId]);

  const goToPreviousRows = useCallback(() => {
    setRowsOffset((current) => Math.max(0, current - FILE_BROWSER_PAGE_LIMIT));
  }, []);

  const goToNextRows = useCallback(() => {
    if (!rowsPage) {
      return;
    }
    setRowsOffset((current) =>
      current + rowsPage.limit < rowsPage.totalCount
        ? current + FILE_BROWSER_PAGE_LIMIT
        : current
    );
  }, [rowsPage]);

  if (!currentCase) {
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
        meta={`当前目录 ${rows?.length ?? 0} 项 / 可执行对象 ${executableCount} 项`}
      >
        <div className="h-10 flex items-center px-4 gap-4 text-xs shrink-0">
          <div className="flex items-center gap-1.5 text-[#666] font-mono text-[11px] min-w-0">
            <HardDrive size={12} />
            {treeNodes.length > 0 ? (
              <>
                <span className="text-[#111] font-semibold truncate max-w-[200px]">
                  {activeRootNode ? displayNodeName(activeRootNode.name, activeRootNode.depth) : '/'}
                </span>
                {currentDirectory &&
                currentDirectory.id !== activeRootNode?.id ? (
                  <>
                    <ChevronRight size={12} className="text-[#aaa]" />
                    <span className="text-[#111] font-semibold truncate max-w-[200px]">
                      {displayNodeName(currentDirectory.name, currentDirectory.depth)}
                    </span>
                  </>
                ) : null}
                {selectedFile ? (
                  <>
                    <ChevronRight size={12} className="text-[#aaa]" />
                    <span className="text-[#111] font-semibold truncate max-w-[200px]">
                      {selectedFile.name}
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
          <FileVisibilityToggle checked={showHidden} onCheckedChange={setShowHidden} />
          <div className="ml-auto text-[#888] text-[11px]">
            显示 {rows?.length ?? 0}
            {rowsPage?.truncated ? ` / ${rowsPage.totalCount}` : ''} 个项目
          </div>
        </div>
      </PageSubbar>

      <div className="flex-1 flex overflow-hidden min-h-0">
        {/* 左侧文件树 */}
        <FileTreePanel
          filteredTreeNodes={filteredTreeNodes}
          activeChildrenPage={activeChildrenPage}
          activeTreeChildrenLoaded={activeTreeChildrenLoaded}
          canLoadMoreTreeChildren={canLoadMoreTreeChildren}
          loadMoreActiveTreeChildren={loadMoreActiveTreeChildren}
          toggleDirectory={toggleDirectory}
          displayNodeName={displayNodeName}
          filterQuery={filterQuery}
          setFilterQuery={setFilterQuery}
          treeWidth={treeWidth}
          isResizing={isResizing}
          onResizeStart={onResizeStart}
          treeContainerRef={treeContainerRef}
          FILE_BROWSER_PAGE_LIMIT={FILE_BROWSER_PAGE_LIMIT}
        />

        {/* 右侧内容区 */}
        <div className="flex-1 flex flex-col min-w-0 min-h-0">
          <FileListPanel
            sortedRows={sortedRows}
            selectedFileId={selectedFileId}
            viewerTab={viewerTab}
            fileSortKey={fileSortKey}
            fileSortDirection={fileSortDirection}
            handleSort={handleSort}
            setSelectedDirectoryId={setSelectedDirectoryId}
            setSelectedFileId={setSelectedFileId}
            setExpandedDirectoryIds={setExpandedDirectoryIds}
            rowsPage={rowsPage}
            canGoToPreviousRows={canGoToPreviousRows}
            canGoToNextRows={canGoToNextRows}
            goToPreviousRows={goToPreviousRows}
            goToNextRows={goToNextRows}
          />

          <FilePreviewPanel
            viewerTab={viewerTab}
            setViewerTab={setViewerTab}
            viewer={viewer}
            onHexJumpInputChange={setJumpOffsetInput}
            onHexJump={jumpToOffset}
            onLoadNextHexRange={loadNextRange}
            onLoadPreviousHexRange={loadPreviousRange}
            textPreview={textPreview}
            imagePreview={imagePreview}
            mediaUrl={mediaUrl}
            selectedFile={selectedFile}
          />
        </div>

        {/* 第三列：对象检查器 */}
        <InspectorPane
          className="hidden lg:flex"
          title="对象检查器"
          subtitle={
            selectedFile
              ? `已选对象 ${selectedFile.name}`
              : '未选中文件对象'
          }
          widthClassName="w-80"
        >
          <div className="space-y-5">
            <InspectorSection title="对象标识">
              <InspectorValue
                value={selectedFile?.name ?? '-'}
                mono
                strong
              />
            </InspectorSection>

            <InspectorSection title="来源路径">
              <InspectorValue
                value={
                  selectedFile?.path ??
                  activeDirectoryPath ??
                  currentDirectory?.name ??
                  '-'
                }
                mono
              />
            </InspectorSection>

            <InspectorSection title="时间戳 (MACB)">
              <div className="font-mono text-[11px] grid grid-cols-[30px_1fr] gap-1">
                <div className="text-[#888]">M</div>
                <div className="text-[#333]">
                  {selectedFile?.modifiedAt ?? '-'}
                </div>
                <div className="text-[#888]">A</div>
                <div className="text-[#333]">
                  {selectedFile?.accessedAt ?? '-'}
                </div>
                <div className="text-[#888]">C</div>
                <div className="text-[#333]">
                  {selectedFile?.changedAt ?? '-'}
                </div>
                <div className="text-[#888]">B</div>
                <div className="text-[#333]">
                  {selectedFile?.createdAt ?? '-'}
                </div>
              </div>
            </InspectorSection>

            <InspectorSection title="摘要字段">
              <InspectorValue
                value={selectedFile?.hashSha256 ?? '-'}
                mono
              />
            </InspectorSection>

            <InspectorSection title="对象状态">
              <div className="font-mono text-[11px] grid grid-cols-[60px_1fr] gap-1">
                <div className="text-[#888]">deleted</div>
                <div className="text-[#333]">{selectedFile?.deleted ? 'true' : 'false'}</div>
                <div className="text-[#888]">hidden</div>
                <div className="text-[#333]">{selectedFile?.hidden ? 'true' : 'false'}</div>
                <div className="text-[#888]">system</div>
                <div className="text-[#333]">{selectedFile?.system ? 'true' : 'false'}</div>
              </div>
            </InspectorSection>

            <InspectorSection title="操作">
              <div className="flex flex-col gap-2">
                <button
                  type="button"
                  onClick={() => {
                    if (selectedFile) {
                      extractFile.mutate(selectedFile);
                    }
                  }}
                  disabled={!selectedFile || extractFile.isPending}
                  className="w-full border border-[#ccc] bg-white text-[#111] hover:bg-[#f0f0f0] py-1.5 text-center text-[11px] rounded-[2px] cursor-pointer font-medium disabled:opacity-50"
                >
                  {extractFile.isPending ? '提取中...' : '提取文件'}
                </button>
                <button
                  onClick={() => {
                    if (selectedFile) {
                      setSelectedTimelineId(selectedFile.id);
                      navigate('/timeline');
                    }
                  }}
                  className="w-full border border-transparent text-[#666] hover:text-[#111] py-1.5 text-center text-[11px] cursor-pointer underline hover:no-underline"
                >
                  在时间线中查看
                </button>
              </div>
            </InspectorSection>
          </div>
        </InspectorPane>
      </div>
    </div>
  );
}
