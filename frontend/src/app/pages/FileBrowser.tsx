import { useEffect, useMemo, useState, useCallback, useRef } from 'react';
import { useNavigate } from 'react-router';
import { ChevronDown, ChevronRight, HardDrive } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { PageSubbar } from '@/components/layout/PageSubbar';
import { useResizablePanel } from '@/hooks/use-resizable-panel';
import {
  InspectorPane,
  InspectorSection,
  InspectorValue,
} from '@/components/layout/InspectorPane';
import { DenseDataTable } from '@/components/tables/DenseDataTable';
import { ViewerTabs } from '@/components/viewers/ViewerTabs';
import { HexViewer } from '@/components/viewers/HexViewer';
import { TextViewer } from '@/components/viewers/TextViewer';
import { ImageViewer } from '@/components/viewers/ImageViewer';
import { VideoViewer } from '@/components/viewers/VideoViewer';
import { AudioViewer } from '@/components/viewers/AudioViewer';
import { TreeConnector } from '@/components/tree/TreeConnector';
import { TreeSearch } from '@/components/tree/TreeSearch';
import { FileIconWithStatusOverlay } from '@/components/files/FileIconWithStatusOverlay';
import { FileVisibilityToggle } from '@/components/files/FileVisibilityToggle';
import { useFileTreeKeyboard } from '@/hooks/use-file-tree-keyboard';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import {
  useExtractFile,
  useFileChildrenPage,
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
  FileEntryRow,
  FileTreeNode,
} from '@/types/models';

function sameTreeNode(left: FileTreeNode, right: FileTreeNode) {
  return (
    left.id === right.id &&
    left.name === right.name &&
    left.depth === right.depth &&
    left.hasChildren === right.hasChildren &&
    left.entryType === right.entryType &&
    left.status === right.status &&
    left.expanded === right.expanded &&
    left.deleted === right.deleted &&
    left.hidden === right.hidden &&
    left.system === right.system
  );
}

function sameTreeNodeList(left: FileTreeNode[], right: FileTreeNode[]) {
  if (left.length !== right.length) {
    return false;
  }

  for (let index = 0; index < left.length; index += 1) {
    if (!sameTreeNode(left[index], right[index])) {
      return false;
    }
  }

  return true;
}

function mergeTreeNodePages(
  existing: FileTreeNode[],
  incoming: FileTreeNode[],
) {
  const merged = [...existing];
  const indexById = new Map(existing.map((node, index) => [node.id, index]));
  let changed = false;

  for (const node of incoming) {
    const existingIndex = indexById.get(node.id);
    if (existingIndex === undefined) {
      merged.push(node);
      indexById.set(node.id, merged.length - 1);
      changed = true;
      continue;
    }

    if (!sameTreeNode(merged[existingIndex], node)) {
      merged[existingIndex] = node;
      changed = true;
    }
  }

  return changed ? merged : existing;
}

function formatBytes(bytes?: number) {
  if (!bytes) {
    return '0 B';
  }
  const units = ['B', 'KB', 'MB', 'GB'];
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024;
    unitIndex += 1;
  }
  return `${value >= 10 || unitIndex === 0 ? Math.round(value) : value.toFixed(1)} ${units[unitIndex]}`;
}

function LargeMediaFallback({
  mediaType,
  previewBytes,
  totalBytes,
  protocol,
}: {
  mediaType: '视频' | '音频';
  previewBytes?: number;
  totalBytes?: number;
  protocol?: boolean;
}) {
  const title = protocol ? `大${mediaType}使用受控流式预览` : `大${mediaType}使用受控分块预览`;
  const detail = protocol
    ? '当前通过 evidence-media 受控协议提供按需 Range 读取；若播放器不支持该协议，可提取文件后在本机播放器查看。'
    : `当前只读取首个 ${formatBytes(previewBytes)} 片段进行安全预览；完整播放需要先使用右侧“提取文件”导出后在本机播放器查看。`;
  return (
    <div className="flex h-full items-center justify-center px-6 text-center text-[#555]">
      <div className="max-w-md space-y-2">
        <div className="text-[12px] font-semibold text-[#222]">
          {title}
        </div>
        <div className="text-[11px] leading-5">
          {detail}
        </div>
        <div className="font-mono text-[10px] text-[#888]">
          total={formatBytes(totalBytes)} / source=opaque handle
        </div>
      </div>
    </div>
  );
}

export function FileBrowser() {
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
  const viewerTab = useUiStore((state) => state.viewerTab);
  const setViewerTab = useUiStore((state) => state.setViewerTab);
  const fileSortKey = useUiStore((state) => state.fileSortKey);
  const fileSortDirection = useUiStore((state) => state.fileSortDirection);
  const setFileSortKey = useUiStore((state) => state.setFileSortKey);
  const toggleFileSortDirection = useUiStore(
    (state) => state.toggleFileSortDirection
  );
  const navigate = useNavigate();
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

  useEffect(() => {
    setRowsOffset(0);
  }, [activeDirectoryId]);

  // 排序变化时回到第一页：排序由后端在完整可见集合上完成，分页随之失效。
  useEffect(() => {
    setRowsOffset(0);
  }, [fileSortKey, fileSortDirection]);

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

    if (selectedDirectoryId && !visibleIds.has(selectedDirectoryId)) {
      setSelectedDirectoryId(rootTree?.[0]?.id);
      setSelectedFileId(undefined);
    }
  }, [
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
      setSelectedFileId(undefined);
    }
  }, [rows, selectedFileId, setSelectedFileId]);

  const selectedFile = rows?.find((row) => row.id === selectedFileId);
  const { data: viewer } = useFileViewer(selectedFile?.id);
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
                <span className="text-[#111] font-semibold">
                  {activeRootNode ? displayNodeName(activeRootNode.name, activeRootNode.depth) : '/'}
                </span>
                {currentDirectory &&
                currentDirectory.id !== activeRootNode?.id ? (
                  <>
                    <ChevronRight size={12} className="text-[#aaa]" />
                    <span className="text-[#111] font-semibold">
                      {displayNodeName(currentDirectory.name, currentDirectory.depth)}
                    </span>
                  </>
                ) : null}
                {selectedFile ? (
                  <>
                    <ChevronRight size={12} className="text-[#aaa]" />
                    <span className="text-[#111] font-semibold">
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
        <div
          className="border-r border-[#e0e0e0] bg-[#fafafa] flex flex-col shrink-0 relative"
          style={{ width: `${treeWidth}px` }}
        >
          {/* 拖拽调整宽度的手柄 */}
          <div
            className={`absolute right-0 top-0 bottom-0 w-1 cursor-col-resize z-10 transition-colors ${
              isResizing ? 'bg-blue-400' : 'hover:bg-blue-200'
            }`}
            onMouseDown={onResizeStart}
            title="拖拽调整宽度"
          />
          <div className="h-7 border-b border-[#e0e0e0] flex items-center px-3 text-[10px] font-semibold text-[#555] uppercase tracking-wider bg-[#f5f5f5]">
            目录树
          </div>
          <TreeSearch onFilter={setFilterQuery} />
          <div
            ref={treeContainerRef}
            className="flex-1 overflow-auto py-1 font-mono text-[11px] select-none"
            tabIndex={0}
          >
            {filteredTreeNodes.length === 0 ? (
              <div className="px-3 py-4 text-[#888]">
                {filterQuery ? '没有匹配的目录。' : '导入数据源后显示目录树。'}
              </div>
            ) : null}
            {activeChildrenPage?.truncated ? (
              <div className="mx-2 mb-1 rounded border border-amber-200 bg-amber-50 px-2 py-1 text-[10px] leading-4 text-amber-800">
                当前目录子目录很多，仅加载前 {activeChildrenPage.limit ?? FILE_BROWSER_PAGE_LIMIT} 个；请用右侧列表或搜索继续定位。
              </div>
            ) : null}
            {filteredTreeNodes.map((node, index) => {
              // 判断是否是父节点的最后一个子节点
              const isLast =
                index === filteredTreeNodes.length - 1 ||
                (filteredTreeNodes[index + 1]?.depth ?? 0) < node.depth;

              return (
                <button
                  key={node.id}
                  type="button"
                  onClick={() => toggleDirectory(node)}
                  className={`w-full flex items-center gap-1 px-2 py-1 cursor-pointer text-left relative ${
                    node.active
                      ? 'bg-[#e0e8f0] text-[#111] font-medium'
                      : 'text-[#555] hover:bg-[#eaeaea]'
                  }`}
                  style={{ paddingLeft: `${8 + node.depth * 16}px` }}
                >
                  {/* 层级连接线 */}
                  {node.depth > 0 && (
                    <TreeConnector depth={node.depth} isLast={isLast} />
                  )}

                  {/* 展开/折叠箭头 */}
                  {node.hasChildren ? (
                    node.expanded ? (
                      <ChevronDown
                        size={12}
                        className="text-[#888] shrink-0"
                      />
                    ) : (
                      <ChevronRight
                        size={12}
                        className="text-[#aaa] shrink-0"
                      />
                    )
                  ) : (
                    <span className="w-3 shrink-0" />
                  )}

                  {/* 文件类型图标 */}
                  <FileIconWithStatusOverlay
                    name={node.name}
                    entryType={node.entryType}
                    status={node.status}
                    expanded={node.expanded}
                    deleted={node.deleted}
                    hidden={node.hidden}
                    system={node.system}
                    size={12}
                  />

                  {/* 文件名 */}
                  <span className="truncate">{displayNodeName(node.name, node.depth)}</span>

                  {/* 状态标签 */}
                  {node.status && node.status !== 'ready' ? (
                    <span className="ml-auto shrink-0 text-[10px] uppercase tracking-wider text-[#888]">
                      {node.status}
                    </span>
                  ) : null}
                </button>
              );
            })}
            {canLoadMoreTreeChildren ? (
              <div className="px-2 py-2">
                <div className="flex items-center justify-between rounded border border-[#e0e0e0] bg-white px-2 py-1.5 text-[10px] text-[#666]">
                  <span>
                    已加载 {activeTreeChildrenLoaded} / {activeChildrenPage?.totalCount ?? activeTreeChildrenLoaded} 个子目录
                  </span>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-6 px-2 text-[10px]"
                    onClick={loadMoreActiveTreeChildren}
                    data-testid="load-more-tree-children"
                  >
                    加载更多子目录
                  </Button>
                </div>
              </div>
            ) : null}
          </div>
        </div>

        {/* 右侧内容区 */}
        <div className="flex-1 flex flex-col min-w-0 min-h-0">
          <div className="flex-1 flex flex-col border-b border-[#e0e0e0] bg-white min-h-0">
            <DenseDataTable<FileEntryRow>
              rows={sortedRows}
              getRowKey={(row) => row.id}
              selectedRowKey={selectedFile?.id}
              onRowClick={(row) => {
                if (row.entryType === 'directory') {
                  setSelectedDirectoryId(row.id);
                  setSelectedFileId(undefined);
                  setExpandedDirectoryIds((current) =>
                    current.includes(row.id)
                      ? current
                      : [...current, row.id]
                  );
                  return;
                }
                setSelectedFileId(row.id);
              }}
              emptyTitle="当前目录为空"
              emptyDescription={
                rowsPage?.truncated
                  ? `当前仅加载前 ${rowsPage.limit} 项，共 ${rowsPage.totalCount} 项。`
                  : '所选路径下没有可展示的文件对象。'
              }
              sortKey={fileSortKey}
              sortDirection={fileSortDirection}
              onSort={handleSort}
              columns={[
                {
                  key: 'name',
                  title: '名称',
                  className: 'w-[34%]',
                  sortable: true,
                  sortKey: 'name',
                  render: (row) => {
                    return (
                      <div className="flex items-center gap-2 min-w-0">
                        <FileIconWithStatusOverlay
                          name={row.name}
                          entryType={row.entryType}
                          deleted={row.deleted}
                          hidden={row.hidden}
                          system={row.system}
                          size={12}
                        />
                        <span className="truncate">{row.name}</span>
                      </div>
                    );
                  },
                },
                {
                  key: 'size',
                  title: '大小',
                  className: 'w-28 text-[#666]',
                  sortable: true,
                  sortKey: 'size',
                  render: (row) =>
                    row.entryType === 'directory'
                      ? '-'
                      : row.size
                        ? `${Math.round(row.size / 1000)} KB`
                        : '0 KB',
                },
                {
                  key: 'modifiedAt',
                  title: '修改时间',
                  className: 'w-44 text-[#666]',
                  sortable: true,
                  sortKey: 'modifiedAt',
                  render: (row) => row.modifiedAt ?? '-',
                },
                {
                  key: 'attr',
                  title: '属性',
                  className: 'text-[#888]',
                  render: (row) =>
                    row.entryType === 'directory'
                      ? 'DIR'
                      : 'A--',
                },
              ]}
            />
            {rowsPage ? (
              <div className="flex items-center justify-between border-t border-[#e0e0e0] bg-[#fafafa] px-3 py-2 text-[11px] text-[#666]">
                <span>
                  显示第 {rowsPage.totalCount === 0 ? 0 : rowsPage.offset + 1} - {Math.min(rowsPage.offset + rowsPage.rows.length, rowsPage.totalCount)} 项，共 {rowsPage.totalCount} 项
                </span>
                <div className="flex items-center gap-2">
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-7 px-2 text-[11px]"
                    onClick={goToPreviousRows}
                    disabled={!canGoToPreviousRows}
                  >
                    上一页
                  </Button>
                  <Button
                    type="button"
                    variant="outline"
                    size="sm"
                    className="h-7 px-2 text-[11px]"
                    onClick={goToNextRows}
                    disabled={!canGoToNextRows}
                  >
                    下一页
                  </Button>
                </div>
              </div>
            ) : null}
          </div>

          <div className="h-72 bg-[#fcfcfc] shrink-0 min-h-0">
            <ViewerTabs
              value={viewerTab}
              onValueChange={(value) =>
                setViewerTab(
                  value as 'metadata' | 'text' | 'hex' | 'preview'
                )
              }
              tabs={[
                {
                  value: 'hex',
                  label: '十六进制',
                  content: viewer?.range.lines?.length ? (
                    <HexViewer lines={viewer.range.lines} />
                  ) : (
                    <div className="text-[#666]">
                      选择文件后显示十六进制预览。
                    </div>
                  ),
                },
                {
                  value: 'text',
                  label: '文本',
                  content: textPreview && !textPreview.isBinary ? (
                    <TextViewer
                      content={textPreview.content}
                      encoding={textPreview.encoding}
                      extension={selectedFile?.ext}
                      isTruncated={textPreview.isTruncated}
                    />
                  ) : (
                    <div className="flex items-center justify-center h-full text-[#888]">
                      {textPreview?.isBinary ? '二进制文件，无法预览文本' : '选择文本文件后显示预览'}
                    </div>
                  ),
                },
                {
                  value: 'preview',
                  label: '预览',
                  content: (() => {
                    const mime = viewer?.handle.mime?.toLowerCase() ?? '';
                    const ext = (selectedFile?.ext ?? selectedFile?.name.split('.').pop() ?? '').toLowerCase().replace(/^\./, '');
                    const imageExts = ['jpg', 'jpeg', 'png', 'gif', 'bmp', 'webp'];
                    const videoExts = ['mp4', 'webm', 'avi', 'mkv'];
                    const audioExts = ['mp3', 'wav', 'flac', 'aac', 'ogg'];
                    
                    // 图片预览
                    if (mime.startsWith('image/') || imageExts.includes(ext)) {
                      if (imagePreview?.dataUrl) {
                        return (
                          <ImageViewer
                            src={imagePreview.dataUrl}
                            mimeType={imagePreview.mimeType}
                            fileName={selectedFile?.name}
                          />
                        );
                      }
                      return <div className="flex items-center justify-center h-full text-[#888]">加载图片预览...</div>;
                    }
                    
                    // 视频预览
                    if (mime.startsWith('video/') || videoExts.includes(ext)) {
                      if (mediaUrl?.url) {
                        return mediaUrl.previewMode === 'rangeFallback' || mediaUrl.previewMode === 'range' ? (
                          <div className="h-full flex flex-col">
                            <div className="flex-1 min-h-0">
                              <VideoViewer
                                src={mediaUrl.url}
                                mimeType={mediaUrl.mimeType}
                                fileName={selectedFile?.name}
                              />
                            </div>
                            <div className="border-t border-[#e0e0e0] bg-[#f8f8f8] px-3 py-1 text-[10px] text-[#666]">
                              受控分块预览: 已读取 {formatBytes(mediaUrl.previewBytes)}，完整播放请提取文件后查看。
                            </div>
                          </div>
                        ) : mediaUrl.previewMode === 'protocol' ? (
                          <div className="h-full flex flex-col">
                            <div className="flex-1 min-h-0">
                              <VideoViewer
                                src={mediaUrl.url}
                                mimeType={mediaUrl.mimeType}
                                fileName={selectedFile?.name}
                              />
                            </div>
                            <div className="border-t border-[#e0e0e0] bg-[#f8f8f8] px-3 py-1 text-[10px] text-[#666]">
                              受控流式预览: evidence-media range 读取，不暴露宿主证据路径。
                            </div>
                          </div>
                        ) : (
                          <VideoViewer
                            src={mediaUrl.url}
                            mimeType={mediaUrl.mimeType}
                            fileName={selectedFile?.name}
                          />
                        );
                      }
                      if (mediaUrl?.canReadRanges || mediaUrl?.handleId) {
                        return (
                          <LargeMediaFallback
                            mediaType="视频"
                            previewBytes={mediaUrl.previewBytes}
                            totalBytes={mediaUrl.size}
                            protocol={mediaUrl.previewMode === 'protocol'}
                          />
                        );
                      }
                      return <div className="flex items-center justify-center h-full text-[#888]">加载视频预览...</div>;
                    }
                    
                    // 音频预览
                    if (mime.startsWith('audio/') || audioExts.includes(ext)) {
                      if (mediaUrl?.url) {
                        return mediaUrl.previewMode === 'rangeFallback' || mediaUrl.previewMode === 'range' ? (
                          <div className="h-full flex flex-col">
                            <div className="flex-1 min-h-0">
                              <AudioViewer
                                src={mediaUrl.url}
                                mimeType={mediaUrl.mimeType}
                                fileName={selectedFile?.name}
                              />
                            </div>
                            <div className="border-t border-[#333] bg-[#111] px-3 py-1 text-[10px] text-[#aaa]">
                              受控分块预览: 已读取 {formatBytes(mediaUrl.previewBytes)}，完整播放请提取文件后查看。
                            </div>
                          </div>
                        ) : mediaUrl.previewMode === 'protocol' ? (
                          <div className="h-full flex flex-col">
                            <div className="flex-1 min-h-0">
                              <AudioViewer
                                src={mediaUrl.url}
                                mimeType={mediaUrl.mimeType}
                                fileName={selectedFile?.name}
                              />
                            </div>
                            <div className="border-t border-[#333] bg-[#111] px-3 py-1 text-[10px] text-[#aaa]">
                              受控流式预览: evidence-media range 读取，不暴露宿主证据路径。
                            </div>
                          </div>
                        ) : (
                          <AudioViewer
                            src={mediaUrl.url}
                            mimeType={mediaUrl.mimeType}
                            fileName={selectedFile?.name}
                          />
                        );
                      }
                      if (mediaUrl?.canReadRanges || mediaUrl?.handleId) {
                        return (
                          <LargeMediaFallback
                            mediaType="音频"
                            previewBytes={mediaUrl.previewBytes}
                            totalBytes={mediaUrl.size}
                            protocol={mediaUrl.previewMode === 'protocol'}
                          />
                        );
                      }
                      return <div className="flex items-center justify-center h-full text-[#888]">加载音频预览...</div>;
                    }
                    
                    // 默认
                    return (
                      <div className="flex items-center justify-center h-full text-[#888]">
                        选择图片、视频或音频文件后显示预览
                      </div>
                    );
                  })(),
                },
                {
                  value: 'metadata',
                  label: '元数据',
                  content: (
                    <div className="space-y-2 font-mono text-[11px] text-[#444]">
                      <div>
                        handle_id: {viewer?.handle.handleId ?? '-'}
                      </div>
                      <div>size: {viewer?.handle.size ?? '-'}</div>
                      <div>mime: {viewer?.handle.mime ?? '-'}</div>
                      <div>path: {selectedFile?.path ?? '-'}</div>
                    </div>
                  ),
                },
              ]}
            />
          </div>
        </div>

        <InspectorPane
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
                      useSelectionStore
                        .getState()
                        .setSelectedTimelineId(selectedFile.id);
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
