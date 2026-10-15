import { useCallback, useEffect, useRef, useState, type MutableRefObject } from 'react';
import { useVirtualizer, type Rect, type Virtualizer } from '@tanstack/react-virtual';
import type { VirtualListProps } from '@/components/lists/virtual-list-types';
function observeViewport(instance: Virtualizer<HTMLDivElement, HTMLDivElement>, emit: (rect: Rect) => void) {
  const element = instance.scrollElement;
  if (!element) return;
  const update = () => emit({ width: element.clientWidth, height: element.clientHeight || 600 });
  update();
  const Observer = instance.targetWindow?.ResizeObserver;
  if (!Observer) return;
  const observer = new Observer(update); observer.observe(element);
  return () => observer.disconnect();
}
export function useVirtualListModel<T>(props: VirtualListProps<T>) {
  const { items, getItemKey, estimateSize = 80, overscan = 4, fixedSize = false, scrollContainerRef, resetKey } = props;
  const viewport = useRef<HTMLDivElement | null>(null);
  const [, setReady] = useState(0);
  const itemKey = useCallback((index: number) => getItemKey(items[index]), [getItemKey, items]);
  const virtualizer = useVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: items.length, getScrollElement: () => viewport.current, getItemKey: itemKey, estimateSize: () => estimateSize,
    overscan, initialRect: { width: 0, height: 600 }, observeElementRect: observeViewport,
    measureElement: fixedSize ? undefined : (element, entry) => entry?.borderBoxSize?.[0]?.blockSize || element.getBoundingClientRect().height || estimateSize,
  });
  const setViewport = useCallback((element: HTMLDivElement | null) => {
    if (viewport.current !== element) setReady((value) => value + 1);
    viewport.current = element;
    if (typeof scrollContainerRef === 'function') scrollContainerRef(element);
    else if (scrollContainerRef) (scrollContainerRef as MutableRefObject<HTMLDivElement | null>).current = element;
  }, [scrollContainerRef]);
  useEffect(() => { if (resetKey === undefined) return; if (viewport.current) viewport.current.scrollTop = 0; virtualizer.scrollToOffset(0); }, [resetKey, virtualizer]);
  return { props, setViewport, totalSize: virtualizer.getTotalSize(), rows: virtualizer.getVirtualItems(), measure: fixedSize ? undefined : virtualizer.measureElement };
}
