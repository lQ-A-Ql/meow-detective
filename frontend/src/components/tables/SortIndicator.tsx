/**
 * SortIndicator - 排序指示器组件
 *
 * 在表头显示排序方向箭头。
 */

import { ArrowUp, ArrowDown, ArrowUpDown } from 'lucide-react';

interface SortIndicatorProps {
  /** 是否是当前排序列 */
  active: boolean;
  /** 排序方向 */
  direction?: 'asc' | 'desc';
}

export function SortIndicator({ active, direction }: SortIndicatorProps) {
  if (!active) {
    return (
      <ArrowUpDown
        size={10}
        className="text-[#ccc] opacity-0 group-hover:opacity-100 transition-opacity"
      />
    );
  }

  return direction === 'asc' ? (
    <ArrowUp size={10} className="text-[#666]" />
  ) : (
    <ArrowDown size={10} className="text-[#666]" />
  );
}
