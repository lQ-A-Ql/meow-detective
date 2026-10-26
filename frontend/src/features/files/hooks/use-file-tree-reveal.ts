import { useEffect, type RefObject } from 'react';
import { TREE_NODE_HEIGHT } from '@/lib/constants';
import type { FileTreeNode } from '@/types/models';

export function useFileTreeReveal(nodes: readonly FileTreeNode[], selectedId: string | undefined,
  viewport: RefObject<HTMLDivElement | null>) {
  useEffect(() => {
    const container = viewport.current;
    const index = nodes.findIndex((node) => node.id === selectedId);
    if (!container || index < 0 || container.clientHeight <= 0) return;
    const top = index * TREE_NODE_HEIGHT;
    const bottom = top + TREE_NODE_HEIGHT;
    if (top < container.scrollTop) container.scrollTop = top;
    else if (bottom > container.scrollTop + container.clientHeight) {
      container.scrollTop = bottom - container.clientHeight;
    }
  }, [nodes, selectedId, viewport]);
}
