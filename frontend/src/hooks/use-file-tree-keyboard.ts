/**
 * use-file-tree-keyboard - 文件树键盘导航 Hook
 *
 * 支持快捷键：
 * - ↑/↓: 上下移动
 * - ←: 折叠目录 / 返回上级
 * - →: 展开目录 / 进入子目录
 * - Enter: 打开文件 / 展开目录
 * - Home: 跳转到第一个
 * - End: 跳转到最后一个
 */

import { useCallback, useEffect, useRef } from 'react';
import { FileTreeNode } from '@/types/models';
import { TREE_NODE_HEIGHT } from '@/lib/constants';

interface UseFileTreeKeyboardOptions {
  /** 扁平化的节点列表 */
  nodes: FileTreeNode[];
  /** 当前激活的节点 ID */
  activeNodeId?: string;
  /** 节点选择回调 */
  onNodeSelect: (nodeId: string) => void;
  /** 节点展开/折叠回调 */
  onNodeToggle: (nodeId: string) => void;
  /** 节点打开回调 */
  onNodeOpen: (nodeId: string) => void;
  /** 滚动容器引用 */
  scrollContainerRef: React.RefObject<HTMLDivElement | null>;
  /** 是否启用 */
  enabled?: boolean;
}

export function useFileTreeKeyboard({
  nodes,
  activeNodeId,
  onNodeSelect,
  onNodeToggle,
  onNodeOpen,
  scrollContainerRef,
  enabled = true,
}: UseFileTreeKeyboardOptions) {
  const focusFrame = useRef<number>();
  // 滚动到指定索引的节点
  const scrollToNode = useCallback(
    (index: number) => {
      const container = scrollContainerRef.current;
      if (!container) return;

      const nodeHeight = TREE_NODE_HEIGHT;
      const nodeTop = index * nodeHeight;
      const nodeBottom = nodeTop + nodeHeight;
      const containerTop = container.scrollTop;
      const containerBottom = containerTop + container.clientHeight;

      if (nodeTop < containerTop) {
        container.scrollTop = nodeTop;
      } else if (nodeBottom > containerBottom) {
        container.scrollTop = nodeBottom - container.clientHeight;
      }
      if (focusFrame.current !== undefined) window.cancelAnimationFrame(focusFrame.current);
      const nodeId = nodes[index]?.id;
      focusFrame.current = window.requestAnimationFrame(() => {
        if (scrollContainerRef.current !== container || !container.isConnected) return;
        const row = [...container.querySelectorAll<HTMLElement>('[role="treeitem"], [data-file-tree-node]')]
          .find((element) => (element.dataset.fileTreeNode ?? element.id) === nodeId);
        row?.focus({ preventScroll: true });
      });
    },
    [nodes, scrollContainerRef]
  );

  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (!enabled || e.defaultPrevented || e.altKey || e.ctrlKey || e.metaKey) return;
      const target = e.target instanceof Element ? e.target : undefined;
      if (target?.closest('input, textarea, select, [contenteditable="true"], [data-tree-toggle]')) return;
      if (!['ArrowDown', 'ArrowUp', 'ArrowRight', 'ArrowLeft', 'Enter', ' ', 'Home', 'End'].includes(e.key)) return;

      const currentIndex = nodes.findIndex((n) => n.id === activeNodeId);
      if (currentIndex === -1 && nodes.length === 0) return;

      const currentNode = nodes[currentIndex];
      e.preventDefault();
      e.stopPropagation();

      switch (e.key) {
        case 'ArrowDown':
          e.preventDefault();
          if (currentIndex < nodes.length - 1) {
            onNodeSelect(nodes[currentIndex + 1].id);
            scrollToNode(currentIndex + 1);
          }
          break;

        case 'ArrowUp':
          e.preventDefault();
          if (currentIndex > 0) {
            onNodeSelect(nodes[currentIndex - 1].id);
            scrollToNode(currentIndex - 1);
          }
          break;

        case 'ArrowRight':
          if (currentNode?.hasChildren && !currentNode.expanded) {
            onNodeToggle(currentNode.id);
          } else if (currentNode?.expanded && nodes[currentIndex + 1]?.depth > currentNode.depth) {
            onNodeSelect(nodes[currentIndex + 1].id);
            scrollToNode(currentIndex + 1);
          }
          break;

        case 'ArrowLeft':
          if (currentNode?.expanded) {
            onNodeToggle(currentNode.id);
          } else if (currentNode) {
            for (let index = currentIndex - 1; index >= 0; index -= 1) {
              if (nodes[index].depth < currentNode.depth) {
                onNodeSelect(nodes[index].id);
                scrollToNode(index);
                break;
              }
            }
          }
          break;

        case 'Enter':
        case ' ':
          e.preventDefault();
          if (currentNode) {
            onNodeOpen(currentNode.id);
          }
          break;

        case 'Home':
          e.preventDefault();
          if (nodes.length > 0) {
            onNodeSelect(nodes[0].id);
            scrollToNode(0);
          }
          break;

        case 'End':
          e.preventDefault();
          if (nodes.length > 0) {
            onNodeSelect(nodes[nodes.length - 1].id);
            scrollToNode(nodes.length - 1);
          }
          break;
      }
    },
    [nodes, activeNodeId, onNodeSelect, onNodeToggle, onNodeOpen, scrollToNode, enabled]
  );

  const handleFocus = useCallback((event: FocusEvent) => {
    const target = event.target instanceof Element ? event.target : undefined;
    const row = target?.closest<HTMLElement>('[role="treeitem"], [data-file-tree-node]');
    const id = row?.dataset.fileTreeNode ?? row?.id;
    if (id && id !== activeNodeId && nodes.some((node) => node.id === id)) onNodeSelect(id);
  }, [activeNodeId, nodes, onNodeSelect]);

  useEffect(() => {
    const container = scrollContainerRef.current;
    if (!container || !enabled) return;

    container.addEventListener('keydown', handleKeyDown);
    container.addEventListener('focusin', handleFocus);
    return () => {
      container.removeEventListener('keydown', handleKeyDown);
      container.removeEventListener('focusin', handleFocus);
    };
  }, [handleFocus, handleKeyDown, scrollContainerRef, enabled]);

  useEffect(() => () => {
    if (focusFrame.current !== undefined) window.cancelAnimationFrame(focusFrame.current);
  }, []);
}
