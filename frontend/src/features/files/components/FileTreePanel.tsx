import { memo, useCallback, type Ref } from 'react';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { TreeConnector } from '@/components/tree/TreeConnector';
import { TreeSearch } from '@/components/tree/TreeSearch';
import { FileIconWithStatusOverlay } from '@/features/files/components/FileIconWithStatusOverlay';
import { FileTreeDataSourceNode } from '@/features/files/components/FileTreeDataSourceNode';
import type { DataSourceSummary, FileTreeNode } from '@/types/models';

export interface FileTreePanelProps {
  filteredTreeNodes: FileTreeNode[];
  activeDirectoryId?: string;
  expandedIds: ReadonlySet<string>;
  activeChildrenPage: { truncated?: boolean; limit?: number; totalCount: number } | undefined;
  activeTreeChildrenLoaded: number;
  canLoadMoreTreeChildren: boolean;
  loadMoreActiveTreeChildren: () => void;
  toggleDirectory: (node: FileTreeNode) => void;
  displayNodeName: (name: string, depth: number, dataSourceId?: string) => string;
  filterQuery: string;
  setFilterQuery: (query: string) => void;
  treeWidth: number;
  isResizing: boolean;
  onResizeStart: (e: React.MouseEvent) => void;
  treeContainerRef: Ref<HTMLDivElement>;
  dataSources?: DataSourceSummary[];
  FILE_BROWSER_PAGE_LIMIT: number;
}

function dataSourceIdFromNodeId(nodeId: string): string {
  return nodeId.replace(/^data-source:/, '');
}

interface TreeNodeRowProps {
  node: FileTreeNode;
  active: boolean;
  expanded: boolean;
  isLast: boolean;
  displayNodeName: (name: string, depth: number, dataSourceId?: string) => string;
  onToggle: (node: FileTreeNode) => void;
}

// Memoized row: node references stay stable across selection/expansion
// changes (see use-file-tree), so only rows whose flags actually flipped
// re-render instead of the whole visible tree.
const TreeNodeRow = memo(function TreeNodeRow({
  node,
  active,
  expanded,
  isLast,
  displayNodeName,
  onToggle,
}: TreeNodeRowProps) {
  const handleClick = useCallback(() => onToggle(node), [onToggle, node]);
  return (
    <Button
      type="button"
      variant="treeControl"
      size="treeRow"
      onClick={handleClick}
      data-active={active ? 'true' : undefined}
      className="relative max-w-full"
    >
      {node.depth > 0 && (
        <TreeConnector depth={node.depth} isLast={isLast} />
      )}
      {node.hasChildren ? (
        expanded ? (
          <ChevronDown size={12} className="text-forensics-muted-light shrink-0" />
        ) : (
          <ChevronRight size={12} className="text-forensics-muted-lighter shrink-0" />
        )
      ) : (
        <span className="w-3 shrink-0" />
      )}
      <FileIconWithStatusOverlay
        name={node.name}
        entryType={node.entryType}
        status={node.status}
        expanded={expanded}
        deleted={node.deleted}
        hidden={node.hidden}
        system={node.system}
        size={12}
      />
      <span className="min-w-0 flex-1 truncate">{displayNodeName(node.name, node.depth, node.dataSourceId)}</span>
      {node.status && node.status !== 'ready' ? (
        <span className="ml-auto shrink-0 text-[10px] uppercase tracking-wider text-forensics-muted-light">
          {node.status}
        </span>
      ) : null}
    </Button>
  );
});

export function FileTreePanel({
  filteredTreeNodes,
  activeDirectoryId,
  expandedIds,
  activeChildrenPage,
  activeTreeChildrenLoaded,
  canLoadMoreTreeChildren,
  loadMoreActiveTreeChildren,
  toggleDirectory,
  displayNodeName,
  filterQuery,
  setFilterQuery,
  treeWidth,
  isResizing,
  onResizeStart,
  treeContainerRef,
  dataSources,
  FILE_BROWSER_PAGE_LIMIT,
}: FileTreePanelProps) {
  return (
    <div
      className="border-r border-forensics-border bg-forensics-panel flex h-full min-h-0 flex-col shrink-0 relative overflow-hidden"
      style={{ width: `${treeWidth}px`, minWidth: `${treeWidth}px`, maxWidth: `${treeWidth}px` }}
    >
      <div
        className={`absolute right-0 top-0 bottom-0 w-1.5 cursor-col-resize z-10 transition-colors ${
          isResizing ? 'bg-forensics-info-bg' : 'hover:bg-forensics-info-bg'
        }`}
        onMouseDown={onResizeStart}
        title="拖拽调整宽度"
      />
      <div className="h-7 shrink-0 border-b border-forensics-border flex items-center px-3 text-[10px] font-light text-forensics-text-tertiary uppercase tracking-wider bg-forensics-panel-strong">
        目录树
      </div>
      <TreeSearch onFilter={setFilterQuery} />
      <ScrollArea
        className="min-h-0 flex-1"
        viewportRef={treeContainerRef}
        viewportClassName="overflow-x-hidden py-1 font-mono text-[11px] select-none"
        viewportProps={{ tabIndex: 0 }}
      >
        {filteredTreeNodes.length === 0 ? (
          <div className="px-3 py-4 text-forensics-muted-light">
            {filterQuery ? '没有匹配的目录。' : '导入数据源后显示目录树。'}
          </div>
        ) : null}
        {activeChildrenPage?.truncated ? (
          <div className="mx-2 mb-1 rounded-none border border-forensics-warning-border bg-forensics-warning-bg px-2 py-1 text-[10px] leading-4 text-forensics-warning-text">
            当前目录子目录很多，仅加载前 {activeChildrenPage.limit ?? FILE_BROWSER_PAGE_LIMIT} 个；请用右侧列表或搜索继续定位。
          </div>
        ) : null}
        {filteredTreeNodes.map((node, index) => {
          const isLast =
            index === filteredTreeNodes.length - 1 ||
            (filteredTreeNodes[index + 1]?.depth ?? 0) < node.depth;
          const isDataSourceNode = node.depth === 0 && node.id.startsWith('data-source:');

          if (isDataSourceNode) {
            const ds = dataSources?.find(
              (d) => d.id === dataSourceIdFromNodeId(node.id),
            );
            return (
              <FileTreeDataSourceNode
                key={node.id}
                node={node}
                expanded={expandedIds.has(node.id)}
                dataSource={ds}
                onClick={() => toggleDirectory(node)}
              />
            );
          }

          return (
            <TreeNodeRow
              key={node.id}
              node={node}
              active={node.id === activeDirectoryId}
              expanded={expandedIds.has(node.id)}
              isLast={isLast}
              displayNodeName={displayNodeName}
              onToggle={toggleDirectory}
            />
          );
        })}
        {canLoadMoreTreeChildren ? (
          <div className="px-2 py-2">
            <div className="flex items-center justify-between rounded-none border border-forensics-border bg-forensics-surface px-2 py-1.5 text-[10px] text-forensics-muted">
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
      </ScrollArea>
    </div>
  );
}
