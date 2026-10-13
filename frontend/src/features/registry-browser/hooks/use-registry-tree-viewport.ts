import { useEffect, useRef } from 'react';
import { useVirtualizer, type Rect, type Virtualizer } from '@tanstack/react-virtual';
import type { RegistryTreeNode } from '../types';
import type { useRegistryTreeKeyboard } from './use-registry-tree-keyboard';
import { REGISTRY_BROWSER_LAYOUT } from '@/design/registry-browser';

function observe(instance: Virtualizer<HTMLDivElement, HTMLDivElement>, emit: (rect: Rect) => void) {
  const viewport = instance.scrollElement;
  if (!viewport) return;
  const update = () => emit({ width: viewport.clientWidth, height: viewport.clientHeight || 600 });
  update();
  const observer = new ResizeObserver(update); observer.observe(viewport);
  return () => observer.disconnect();
}

export function useRegistryTreeViewport(rows: RegistryTreeNode[], keyboard: ReturnType<typeof useRegistryTreeKeyboard>) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer<HTMLDivElement, HTMLDivElement>({ count: rows.length,
    getScrollElement: () => viewportRef.current, getItemKey: (index) => rows[index].id,
    estimateSize: (index) => REGISTRY_BROWSER_LAYOUT.treeRowPixels + (rows[index].nextCursor || rows[index].error || rows[index].loading ? 32 : 0),
    overscan: 8, initialRect: { width: 0, height: 600 }, observeElementRect: observe });
  useEffect(() => {
    if (!keyboard.focusRequest.current) return;
    keyboard.focusRequest.current = false;
    const index = rows.findIndex((row) => row.id === keyboard.focusedId);
    if (index < 0) return;
    virtualizer.scrollToIndex(index, { align: 'auto' });
    const frame = requestAnimationFrame(() => document.getElementById(rows[index].domId)?.focus({ preventScroll: true }));
    return () => cancelAnimationFrame(frame);
  }, [keyboard.focusedId, rows, virtualizer, keyboard.focusRequest]);
  return { viewportRef, virtualizer, rows };
}
