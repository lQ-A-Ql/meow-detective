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
import { Button } from '@/app/components/ui/button';
import { HorizontalScroll } from '@/components/layout/HorizontalScroll';
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
  /** 接近当前数据末尾时请求下一段。 */
  onReachEnd?: () => void;
  /** 逻辑数据上下文变化时重置滚动位置与续载去重状态。 */
  loadContextKey?: string;
  /** 查询缓存完成一次更新时变化，用于解除被取消请求留下的续载锁。 */
  loadStateKey?: string | number;
  hasMore?: boolean;
  loadingMore?: boolean;
  loadMoreFailed?: boolean;
  loadMoreErrorText?: string;
  retryLoadMoreLabel?: string;
  /** 续载失败后的恢复动作；游标分页通常应重建查询链，而非重放旧游标。 */
  onRetryLoadMore?: () => unknown;
  initialLoadFailed?: boolean;
  initialLoadErrorText?: string;
  retryInitialLoadLabel?: string;
  onRetryInitialLoad?: () => void;
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
const AUTOMATIC_RETRY_DELAY_MS = 500;

/**
 * 表格行组件 (使用 memo 优化)
 */
function TableRowMemoBase<T>({
  row,
  columns,
  selected,
  onRowClick,
}: TableRowMemoProps<T>) {
  const handleRowClick = () => {
    if (typeof window !== 'undefined') {
      const selection = window.getSelection();
      if (selection && !selection.isCollapsed) {
        return;
      }
    }
    onRowClick?.(row);
  };

  return (
    <TableRow
      data-state={selected ? 'selected' : undefined}
      className={`h-[31px] cursor-pointer border-b ${
        selected
          ? 'bg-forensics-sakura-150 text-forensics-text'
          : 'text-forensics-text-secondary hover:bg-forensics-hover'
      }`}
      onClick={handleRowClick}
    >
      {columns.map((column, index) => (
        <TableCell
          key={column.key}
          className={`h-[31px] px-2 py-1.5 align-middle ${
            index < columns.length - 1
              ? 'border-r border-forensics-border-light'
              : ''
          } ${column.className ?? ''}`}
        >
          <HorizontalScroll
            variant="cell"
            revealOnHover
            className="whitespace-nowrap text-inherit"
          >
            <div className="min-w-full w-max select-text pr-2">
              {column.render(row)}
            </div>
          </HorizontalScroll>
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
  onReachEnd,
  loadContextKey,
  loadStateKey,
  hasMore = false,
  loadingMore = false,
  loadMoreFailed = false,
  loadMoreErrorText = '加载更多记录失败。',
  retryLoadMoreLabel = '重试',
  onRetryLoadMore,
  initialLoadFailed = false,
  initialLoadErrorText = '记录加载失败。',
  retryInitialLoadLabel = '重试',
  onRetryInitialLoad,
}: DenseDataTableProps<T>) {
  const containerRef = useRef<HTMLDivElement>(null);
  const requestedRowCountRef = useRef<number | undefined>(undefined);
  const automaticRetryKeyRef = useRef<string | undefined>(undefined);
  const automaticRetryTimerRef = useRef<number | undefined>(undefined);
  const retryInFlightRef = useRef(false);
  const wasLoadingMoreRef = useRef(false);
  const [scrollTop, setScrollTop] = useState(0);
  const [containerHeight, setContainerHeight] = useState(DEFAULT_CONTAINER_HEIGHT);

  const handleSort = useCallback(
    (key: string) => {
      onSort?.(key);
    },
    [onSort]
  );

  const requestMore = useCallback(() => {
    if (
      !hasMore
      || loadingMore
      || loadMoreFailed
      || !onReachEnd
      || requestedRowCountRef.current === rows.length
    ) {
      return;
    }

    requestedRowCountRef.current = rows.length;
    onReachEnd();
  }, [hasMore, loadMoreFailed, loadingMore, onReachEnd, rows.length]);

  const runLoadMoreRecovery = useCallback(() => {
    const recovery = onRetryLoadMore ?? onReachEnd;
    if (
      !hasMore
      || loadingMore
      || !recovery
      || retryInFlightRef.current
    ) {
      return;
    }

    retryInFlightRef.current = true;
    requestedRowCountRef.current = rows.length;
    let result: unknown;
    try {
      result = recovery();
    } catch {
      retryInFlightRef.current = false;
      return;
    }
    void Promise.resolve(result).then(
      () => { retryInFlightRef.current = false; },
      () => { retryInFlightRef.current = false; },
    );
  }, [hasMore, loadingMore, onReachEnd, onRetryLoadMore, rows.length]);

  const retryMore = useCallback(() => {
    if (automaticRetryTimerRef.current !== undefined) {
      window.clearTimeout(automaticRetryTimerRef.current);
      automaticRetryTimerRef.current = undefined;
    }
    runLoadMoreRecovery();
  }, [runLoadMoreRecovery]);

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

  useEffect(() => {
    requestedRowCountRef.current = undefined;
    automaticRetryKeyRef.current = undefined;
    retryInFlightRef.current = false;
    if (automaticRetryTimerRef.current !== undefined) {
      window.clearTimeout(automaticRetryTimerRef.current);
      automaticRetryTimerRef.current = undefined;
    }
    setScrollTop(0);

    if (containerRef.current) {
      containerRef.current.scrollTop = 0;
    }
  }, [loadContextKey]);

  useEffect(() => {
    requestedRowCountRef.current = undefined;
    automaticRetryKeyRef.current = undefined;
    retryInFlightRef.current = false;
  }, [loadStateKey]);

  useEffect(() => {
    if (loadingMore) {
      wasLoadingMoreRef.current = true;
      return;
    }
    if (wasLoadingMoreRef.current) {
      wasLoadingMoreRef.current = false;
      requestedRowCountRef.current = undefined;
    }
  }, [loadingMore]);

  useEffect(() => {
    if (!loadingMore && loadMoreFailed) {
      requestedRowCountRef.current = undefined;
    }
  }, [loadMoreFailed, loadingMore]);

  useEffect(() => {
    if (!loadMoreFailed || loadingMore || !hasMore || !(onRetryLoadMore ?? onReachEnd)) return;

    const retryKey = `${loadContextKey ?? ''}:${rows.length}`;
    if (automaticRetryKeyRef.current === retryKey) return;
    automaticRetryKeyRef.current = retryKey;

    automaticRetryTimerRef.current = window.setTimeout(() => {
      automaticRetryTimerRef.current = undefined;
      runLoadMoreRecovery();
    }, AUTOMATIC_RETRY_DELAY_MS);
    return () => {
      if (automaticRetryTimerRef.current !== undefined) {
        window.clearTimeout(automaticRetryTimerRef.current);
        automaticRetryTimerRef.current = undefined;
      }
    };
  }, [
    hasMore,
    loadContextKey,
    loadMoreFailed,
    loadingMore,
    onReachEnd,
    onRetryLoadMore,
    runLoadMoreRecovery,
    rows.length,
  ]);

  const handleScroll = useCallback((event: React.UIEvent<HTMLDivElement>) => {
    const container = event.currentTarget;
    setScrollTop(container.scrollTop);
    const remaining = container.scrollHeight - container.scrollTop - container.clientHeight;
    if (remaining <= ROW_HEIGHT * OVERSCAN_ROWS) {
      requestMore();
    }
  }, [requestMore]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container || !hasMore || loadingMore || loadMoreFailed || !onReachEnd) return;
    if (container.clientHeight <= 0) return;

    const remaining = container.scrollHeight - container.scrollTop - container.clientHeight;
    if (remaining > ROW_HEIGHT * OVERSCAN_ROWS) return;
    requestMore();
  }, [
    containerHeight,
    hasMore,
    loadContextKey,
    loadMoreFailed,
    loadingMore,
    onReachEnd,
    requestMore,
    rows.length,
  ]);

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
      className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden bg-transparent font-mono text-[11px]"
      onScroll={handleScroll}
    >
      <Table className="text-[11px]">
        <TableHeader className="sticky top-0 z-10 bg-forensics-panel">
          <TableRow className="border-b border-forensics-border hover:bg-forensics-panel">
            {columns.map((column) => (
              <TableHead
                key={column.key}
                className={`group h-7 border-r border-forensics-border px-2 text-[11px] font-light tracking-wide text-forensics-text-tertiary last:border-r-0 ${
                  column.sortable ? 'cursor-pointer select-none hover:bg-forensics-hover' : ''
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
          {rows.length === 0 && initialLoadFailed ? (
            <TableRow aria-live="polite" className="hover:bg-transparent">
              <TableCell colSpan={columns.length} className="px-4 py-8">
                <div className="flex items-center justify-center gap-2 text-forensics-error-text">
                  <span>{initialLoadErrorText}</span>
                  {onRetryInitialLoad ? (
                    <Button
                      type="button"
                      variant="forensicsOutline"
                      size="compact"
                      onClick={onRetryInitialLoad}
                    >
                      {retryInitialLoadLabel}
                    </Button>
                  ) : null}
                </div>
              </TableCell>
            </TableRow>
          ) : null}
          {rows.length === 0 && !initialLoadFailed ? (
            <TableRow className="hover:bg-transparent">
              <TableCell colSpan={columns.length} className="px-4 py-8">
                <div className="space-y-1 text-center font-serif">
                  <div className="text-[12px] font-light text-forensics-text">
                    {emptyTitle}
                  </div>
                  <div className="text-[11px] text-forensics-muted">
                    {emptyDescription}
                  </div>
                </div>
              </TableCell>
            </TableRow>
          ) : null}
          {rows.length > 0 && topSpacerHeight > 0 ? (
            <TableRow aria-hidden="true" className="hover:bg-transparent">
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
            <TableRow aria-hidden="true" className="hover:bg-transparent">
              <TableCell
                colSpan={columns.length}
                className="border-r-0 p-0"
                style={{ height: bottomSpacerHeight }}
              />
            </TableRow>
          ) : null}
          {loadingMore ? (
            <TableRow aria-live="polite" className="hover:bg-transparent">
              <TableCell
                colSpan={columns.length}
                className="border-r-0 px-3 py-2 text-center text-forensics-muted"
              >
                正在加载更多记录...
              </TableCell>
            </TableRow>
          ) : null}
          {loadMoreFailed && !loadingMore ? (
            <TableRow aria-live="polite" className="hover:bg-transparent">
              <TableCell
                colSpan={columns.length}
                className="border-r-0 px-3 py-2 text-center text-forensics-error-text"
              >
                <div className="flex items-center justify-center gap-2">
                  <span>{loadMoreErrorText}</span>
                  <Button
                    type="button"
                    variant="forensicsOutline"
                    size="compact"
                    onClick={retryMore}
                  >
                    {retryLoadMoreLabel}
                  </Button>
                </div>
              </TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>
    </div>
  );
}
