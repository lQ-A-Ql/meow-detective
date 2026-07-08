/**
 * VirtualFileTree - 虚拟滚动文件树组件
 *
 * 使用虚拟滚动优化大量节点的渲染性能。
 * 只渲染可见区域的节点，大幅提升性能。
 */

import { useRef, useCallback } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { ChevronDown, ChevronRight } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
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
}

export function VirtualFileTree({
  nodes,
  onNodeClick,
  itemSize = 28,
  overscan = 10,
}: VirtualFileTreeProps) {
  const parentRef = useRef<HTMLDivElement>(null);

  const virtualizer = useVirtualizer({
    count: nodes.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => itemSize,
    overscan,
  });

  const handleClick = useCallback(
    (node: FileTreeNode) => {
      onNodeClick(node);
    },
    [onNodeClick]
  );

  return (
    <div
      ref={parentRef}
      className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden"
      style={{ contain: 'strict' }}
    >
      <div
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualizer.getVirtualItems().map((virtualRow) => {
          const node = nodes[virtualRow.index];
          if (!node) return null;

          const isLast =
            virtualRow.index === nodes.length - 1 ||
            (nodes[virtualRow.index + 1]?.depth ?? 0) < node.depth;

          return (
            <div
              key={node.id}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                height: `${virtualRow.size}px`,
                transform: `translateY(${virtualRow.start}px)`,
              }}
            >
              <Button
                type="button"
                variant="treeControl"
                size="treeRow"
                onClick={() => handleClick(node)}
                data-active={node.active ? 'true' : undefined}
                className="h-full max-w-full"
                style={{ paddingLeft: `${8 + node.depth * 16}px` }}
              >
                {/* 层级连接线 */}
                {node.depth > 0 && (
                  <TreeConnector depth={node.depth} isLast={isLast} />
                )}

                {/* 展开/折叠箭头 */}
                {node.hasChildren ? (
                  node.expanded ? (
                    <ChevronDown size={12} className="text-[#888] shrink-0" />
                  ) : (
                    <ChevronRight size={12} className="text-[#aaa] shrink-0" />
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
                <span className="min-w-0 flex-1 truncate">{node.name}</span>

                {/* 状态标签 */}
                {node.status && node.status !== 'ready' ? (
                  <span className="ml-auto shrink-0 text-[10px] uppercase tracking-wider text-[#888]">
                    {node.status}
                  </span>
                ) : null}
              </Button>
            </div>
          );
        })}
      </div>
    </div>
  );
}
