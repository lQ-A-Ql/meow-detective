/**
 * DenseDataTable - 密集数据表格组件 (性能优化版)
 *
 * 优化点：
 * 1. 使用 React.memo 减少不必要的重渲染
 * 2. 使用 useCallback 缓存事件处理
 * 3. 虚拟滚动支持大列表
 */

import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactElement,
  type ReactNode,
} from 'react';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/app/components/ui/table';
import { SortIndicator } from './SortIndicator';

export interface DenseColumn<T> {
  key: string;
  title: ReactNode;
  className?: string;
  /** 是否可排序 */
  sortable?: boolean;
  /** 排序键 (用于回调) */
  sortKey?: string;
  render: (row: T) => ReactNode;
}

interface DenseDataTableProps<T> {
  columns: DenseColumn<T>[];
  rows: T[];
  getRowKey: (row: T) => string;
  selectedRowKey?: string;
  onRowClick?: (row: T) => void;
  emptyTitle?: string;
  emptyDescription?: string;
  /** 当前排序键 */
  sortKey?: string;
  /** 排序方向 */
  sortDirection?: 'asc' | 'desc';
  /** 排序回调 */
  onSort?: (key: string) => void;
}

interface TableRowMemoProps<T> {
  row: T;
  columns: DenseColumn<T>[];
  selected: boolean;
  onRowClick?: (row: T) => void;
}

const ROW_HEIGHT = 31;
const OVERSCAN_ROWS = 8;
const DEFAULT_CONTAINER_HEIGHT = 600;

/**
 * 表格行组件 (使用 memo 优化)
 */
function TableRowMemoBase<T>({
  row,
  columns,
  selected,
  onRowClick,
}: TableRowMemoProps<T>) {
  return (
    <TableRow
      data-state={selected ? 'selected' : undefined}
      className={`h-[31px] cursor-pointer border-b ${
        selected
          ? 'bg-[#e8e8e8] text-[#111]'
          : 'text-[#333] hover:bg-[#f9f9f9]'
      }`}
      onClick={() => onRowClick?.(row)}
    >
      {columns.map((column, index) => (
        <TableCell
          key={column.key}
          className={`h-[31px] max-w-0 overflow-hidden whitespace-nowrap px-2 py-1.5 align-middle ${
            index < columns.length - 1
              ? 'border-r border-[#f0f0f0]'
              : ''
          } ${column.className ?? ''}`}
        >
          <div className="truncate">{column.render(row)}</div>
        </TableCell>
      ))}
    </TableRow>
  );
}

const TableRowMemo = memo(TableRowMemoBase) as <T>(
  props: TableRowMemoProps<T>
) => ReactElement;

export function DenseDataTable<T>({
  columns,
  rows,
  getRowKey,
  selectedRowKey,
  onRowClick,
  emptyTitle = '暂无记录',
  emptyDescription = '当前范围内没有可显示的数据。',
  sortKey,
  sortDirection,
  onSort,
}: DenseDataTableProps<T>) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [containerHeight, setContainerHeight] = useState(DEFAULT_CONTAINER_HEIGHT);

  const handleSort = useCallback(
    (key: string) => {
      onSort?.(key);
    },
    [onSort]
  );

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    const updateContainerHeight = () => {
      const nextHeight = container.clientHeight;
      if (nextHeight > 0) {
        setContainerHeight(nextHeight);
      }
    };

    updateContainerHeight();

    if (typeof ResizeObserver === 'undefined') return;

    const observer = new ResizeObserver(() => {
      updateContainerHeight();
    });

    observer.observe(container);
    return () => observer.disconnect();
  }, []);

  const handleScroll = useCallback((event: React.UIEvent<HTMLDivElement>) => {
    setScrollTop(event.currentTarget.scrollTop);
  }, []);

  const visibleRange = useMemo(() => {
    const startIndex = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN_ROWS);
    const visibleRowCount = Math.ceil(containerHeight / ROW_HEIGHT) + OVERSCAN_ROWS * 2;
    const endIndex = Math.min(rows.length, startIndex + visibleRowCount);

    return { startIndex, endIndex };
  }, [containerHeight, rows.length, scrollTop]);

  const visibleRows = useMemo(
    () => rows.slice(visibleRange.startIndex, visibleRange.endIndex),
    [rows, visibleRange.endIndex, visibleRange.startIndex]
  );

  const topSpacerHeight = visibleRange.startIndex * ROW_HEIGHT;
  const bottomSpacerHeight = Math.max(
    0,
    (rows.length - visibleRange.endIndex) * ROW_HEIGHT
  );

  return (
    <div
      ref={containerRef}
      className="min-h-0 flex-1 overflow-auto bg-white font-mono text-[11px]"
      onScroll={handleScroll}
    >
      <Table className="text-[11px]">
        <TableHeader className="sticky top-0 z-10 bg-[#fafafa]">
          <TableRow className="border-b border-[#e0e0e0] hover:bg-[#fafafa]">
            {columns.map((column) => (
              <TableHead
                key={column.key}
                className={`group h-7 border-r border-[#e0e0e0] px-2 text-[11px] font-medium tracking-wider text-[#555] last:border-r-0 ${
                  column.sortable ? 'cursor-pointer select-none hover:bg-[#f0f0f0]' : ''
                } ${column.className ?? ''}`}
                onClick={() => column.sortable && handleSort(column.sortKey ?? column.key)}
              >
                <div className="flex items-center gap-1">
                  <span>{column.title}</span>
                  {column.sortable && (
                    <SortIndicator
                      active={sortKey === (column.sortKey ?? column.key)}
                      direction={sortDirection}
                    />
                  )}
                </div>
              </TableHead>
            ))}
          </TableRow>
        </TableHeader>
        <TableBody>
          {rows.length === 0 ? (
            <TableRow className="hover:bg-white">
              <TableCell colSpan={columns.length} className="px-4 py-8">
                <div className="space-y-1 text-center font-sans">
                  <div className="text-[12px] font-medium text-[#222]">
                    {emptyTitle}
                  </div>
                  <div className="text-[11px] text-[#777]">
                    {emptyDescription}
                  </div>
                </div>
              </TableCell>
            </TableRow>
          ) : null}
          {rows.length > 0 && topSpacerHeight > 0 ? (
            <TableRow aria-hidden="true" className="hover:bg-white">
              <TableCell
                colSpan={columns.length}
                className="border-r-0 p-0"
                style={{ height: topSpacerHeight }}
              />
            </TableRow>
          ) : null}
          {visibleRows.map((row) => {
            const key = getRowKey(row);
            const selected = key === selectedRowKey;
            return (
              <TableRowMemo
                key={key}
                row={row}
                columns={columns}
                selected={selected}
                onRowClick={onRowClick}
              />
            );
          })}
          {rows.length > 0 && bottomSpacerHeight > 0 ? (
            <TableRow aria-hidden="true" className="hover:bg-white">
              <TableCell
                colSpan={columns.length}
                className="border-r-0 p-0"
                style={{ height: bottomSpacerHeight }}
              />
            </TableRow>
          ) : null}
        </TableBody>
      </Table>
    </div>
  );
}
