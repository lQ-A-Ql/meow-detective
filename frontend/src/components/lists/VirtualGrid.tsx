import { useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import { VirtualList } from './VirtualList';

interface VirtualGridProps<T> {
  items: readonly T[];
  getItemKey: (item: T) => string;
  renderItem: (item: T) => ReactNode;
  resetKey?: string;
}

const getRowKey = <T,>(row: { key: string; items: T[] }) => row.key;

/** Responsive cards with measured virtual rows; all cards remain reachable. */
export function VirtualGrid<T>({ items, getItemKey, renderItem, resetKey }: VirtualGridProps<T>) {
  const container = useRef<HTMLDivElement>(null);
  const [columns, setColumns] = useState(1);
  useEffect(() => {
    const element = container.current;
    if (!element) return;
    const update = () => {
      const width = element.clientWidth;
      setColumns(width >= 1280 ? 3 : width >= 640 ? 2 : 1);
    };
    update();
    const observer = new ResizeObserver(update);
    observer.observe(element);
    return () => observer.disconnect();
  }, []);
  const rows = useMemo(() => {
    const result: Array<{ key: string; items: T[] }> = [];
    for (let index = 0; index < items.length; index += columns) {
      const group = items.slice(index, index + columns);
      result.push({ key: JSON.stringify(group.map(getItemKey)), items: group });
    }
    return result;
  }, [items, columns, getItemKey]);
  return (
    <div ref={container}>
      <VirtualList items={rows} getItemKey={getRowKey} estimateSize={260}
        style={{ height: 'min(60vh, 40rem)' }} resetKey={`${resetKey ?? ''}:${columns}`}
        renderItem={(row) => (
          <div className="grid gap-3 pb-3" style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}>
            {row.items.map((item) => <div key={getItemKey(item)} className="min-w-0">{renderItem(item)}</div>)}
          </div>
        )} />
    </div>
  );
}
