import type { CSSProperties, ReactNode, Ref } from 'react';
export interface VirtualListProps<T> {
  items: readonly T[]; getItemKey: (item: T) => string; renderItem: (item: T, index: number) => ReactNode;
  estimateSize?: number; overscan?: number; fixedSize?: boolean; className?: string; viewportClassName?: string;
  style?: CSSProperties; resetKey?: string; ariaLabel?: string; scrollContainerRef?: Ref<HTMLDivElement>;
  viewportTabIndex?: number; treeContent?: boolean;
}
