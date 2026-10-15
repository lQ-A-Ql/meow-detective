/**
 * VirtualFileTree - 虚拟滚动文件树组件
 *
 * 使用虚拟滚动优化大量节点的渲染性能。
 * 只渲染可见区域的节点，大幅提升性能。
 */

import type { ReactNode, Ref } from 'react';
import { VirtualList } from '@/components/lists/VirtualList';
import { TreeRow } from '@/components/tree/TreeRow';




import { TreeConnector } from '@/components/tree/TreeConnector';
import { FileIconWithStatusOverlay } from '@/features/files/components/FileIconWithStatusOverlay';
import type { FileTreeNode } from '@/types/models';

interface VirtualFileTreeProps {
  /** 扁平化的节点列表 */
  nodes: Array<FileTreeNode & { active?: boolean; expanded?: boolean }>;
  /** 节点点击回调 */
  onNodeClick: (node: FileTreeNode) => void;
  /** 每行高度 */
  itemSize?: number;
  /** 预渲染行数 */
  overscan?: number;
  /** 将列表行交给调用方绘制，用于保留数据源等特殊节点样式。 */
  renderNode?: (node: FileTreeNode & { active?: boolean; expanded?: boolean }, isLast: boolean) => ReactNode;
  /** 外部键盘导航使用的滚动容器引用。 */
  scrollContainerRef?: Ref<HTMLDivElement>;
  viewportClassName?: string;
  viewportTabIndex?: number;
}

export function VirtualFileTree({
  nodes,
  onNodeClick,
  itemSize = 28,
  overscan = 10,
  renderNode,
  scrollContainerRef,
  viewportClassName,
  viewportTabIndex,
}: VirtualFileTreeProps) {
  return <VirtualList items={nodes} getItemKey={(node) => node.id} estimateSize={itemSize} overscan={overscan} fixedSize treeContent className="min-h-0 flex-1" scrollContainerRef={scrollContainerRef} viewportClassName={viewportClassName ?? 'overflow-x-hidden'} viewportTabIndex={viewportTabIndex}
    renderItem={(node, index) => {

          if (!node) return null;

          const isLast =
            index === nodes.length - 1 ||
            (nodes[index + 1]?.depth ?? 0) < node.depth;

          return <div className="h-full">
              {renderNode ? renderNode(node, isLast) : (
              <TreeRow
                depth={node.depth}
                expanded={node.expanded}
                hasChildren={node.hasChildren}
                selected={node.active}
                active={node.active}
                ariaLabel={node.name}
                onActivate={() => onNodeClick(node)}
                onToggle={() => onNodeClick(node)}
                toggleLabel={node.name}
                className="h-full"
                style={{ paddingLeft: `${8 + node.depth * 16}px` }}
                connector={node.depth > 0 ? <TreeConnector depth={node.depth} isLast={isLast} /> : null}
                icon={
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
                }
                label={node.name}
                meta={
                  node.status && node.status !== 'ready' ? (
                    <span className="ml-auto shrink-0 text-[10px] uppercase tracking-wider text-forensics-ink-3">
                      {node.status}
                    </span>
                  ) : null
                }
              />
              )}
          </div>;
        }} />;
}
