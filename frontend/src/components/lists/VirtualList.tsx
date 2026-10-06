import { useCallback, useEffect, useRef, type CSSProperties, type ReactNode } from 'react';
import { useVirtualizer, type Rect, type Virtualizer } from '@tanstack/react-virtual';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { cn } from '@/app/components/ui/utils';

interface VirtualListProps<T> {
  items: readonly T[];
  getItemKey: (item: T) => string;
  renderItem: (item: T) => ReactNode;
  estimateSize?: number;
  className?: string;
  viewportClassName?: string;
  style?: CSSProperties;
  resetKey?: string;
  ariaLabel?: string;
}

function observeViewport(instance: Virtualizer<HTMLDivElement, HTMLDivElement>, emit: (rect: Rect) => void) {
  const element = instance.scrollElement;
  if (!element) return;
  const update = () => emit({ width: element.clientWidth, height: element.clientHeight || 600 });
  update();
  const ResizeObserverImpl = instance.targetWindow?.ResizeObserver;
  if (!ResizeObserverImpl) return;
  const observer = new ResizeObserverImpl(update);
  observer.observe(element);
  return () => observer.disconnect();
}

/** Bounded DOM window for variable-height evidence lists. Callers provide a
 * definite height or flex viewport; measurement follows wrapping and resizing. */
export function VirtualList<T>({
  items, getItemKey, renderItem, estimateSize = 80, className,
  viewportClassName, style, resetKey, ariaLabel,
}: VirtualListProps<T>) {
  const viewport = useRef<HTMLDivElement>(null);
  const itemKey = useCallback((index: number) => getItemKey(items[index]), [getItemKey, items]);
  const virtualizer = useVirtualizer<HTMLDivElement, HTMLDivElement>({
    count: items.length,
    getScrollElement: () => viewport.current,
    getItemKey: itemKey,
    estimateSize: () => estimateSize,
    overscan: 4,
    initialRect: { width: 0, height: 600 },
    observeElementRect: observeViewport,
    measureElement: (element, entry) => entry?.borderBoxSize?.[0]?.blockSize
      || element.getBoundingClientRect().height || estimateSize,
  });

  useEffect(() => {
    if (viewport.current) viewport.current.scrollTop = 0;
    virtualizer.scrollToOffset(0);
  }, [resetKey, virtualizer]);

  return (
    <ScrollArea className={cn('min-h-0 overflow-hidden', className)} style={style}
      viewportRef={viewport} viewportClassName={viewportClassName}
      viewportProps={{ role: 'list', 'aria-label': ariaLabel, tabIndex: 0 }}>
      <div className="relative w-full" style={{ height: virtualizer.getTotalSize() }}>
        {virtualizer.getVirtualItems().map((row) => (
          <div key={row.key} data-index={row.index} ref={virtualizer.measureElement}
            role="listitem" aria-posinset={row.index + 1} aria-setsize={items.length}
            className="absolute left-0 top-0 w-full" style={{ transform: `translateY(${row.start}px)` }}>
            {renderItem(items[row.index])}
          </div>
        ))}
      </div>
    </ScrollArea>
  );
}
