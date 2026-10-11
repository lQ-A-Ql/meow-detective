import { RefreshCw } from 'lucide-react';
/**
 * DenseDataTable - 密集数据表格组件 (性能优化版)
 *
 * 优化点：
 * 1. 使用 React.memo 减少不必要的重渲染
 * 2. 使用 useCallback 缓存事件处理
 * 3. 虚拟滚动支持大列表
 */

import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactElement,
  type ReactNode,
} from 'react';
import { useTranslation } from 'react-i18next';
import {
  useVirtualizer,
  type Rect,
  type Virtualizer,
} from '@tanstack/react-virtual';
import {
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@/app/components/ui/table';
import { Button } from '@/app/components/ui/button';
import { toast } from '@/app/components/ui/toast';
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { DenseDataTableRow } from './DenseDataTableRow';
import { DENSE_TABLE_ROW_HEIGHT } from './dense-table-metrics';
import { SortIndicator } from './SortIndicator';
import { DenseTableFilterBar } from './DenseTableFilterBar';
import { DenseTableStatusRows } from './DenseTableStatusRows';
import { useDenseTableFilter } from './useDenseTableFilter';

export interface DenseColumn<T> {
  key: string;
  title: ReactNode;
  className?: string;
  /** 是否可排序 */
  sortable?: boolean;
  /** 排序键 (用于回调) */
  sortKey?: string;
  /** 搜索/列筛选用的文本提取；不提供则该列不参与匹配。 */
  text?: (row: T) => string;
  /** 该列出现在工具条的列筛选下拉中（需要同时提供 text）。 */
  filterable?: boolean;
  render: (row: T) => ReactNode;
}

interface DenseDataTableProps<T> {
  columns: DenseColumn<T>[];
  rows: T[];
  getRowKey: (row: T) => string;
  selectedRowKey?: string;
  onRowClick?: (row: T) => void;
  onRowDoubleClick?: (row: T) => void;
  renderRowContextMenu?: (row: T, trigger: ReactElement) => ReactElement;
  emptyTitle?: string;
  emptyDescription?: string;
  /** Optional next step rendered inside the empty state. */
  emptyAction?: ReactNode;
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
  /** 为动态多列表格保留最小列宽，并允许整个表格横向滚动。 */
  horizontalScroll?: boolean;
  minColumnWidth?: number;
  /** 开启客户端搜索/列筛选工具条；过滤只作用于当前已加载行。 */
  filterable?: boolean;
}

const OVERSCAN_ROWS = 8;
const DEFAULT_CONTAINER_HEIGHT = 600;
const AUTOMATIC_RETRY_DELAY_MS = 500;

function observeTableViewport(
  instance: Virtualizer<HTMLDivElement, Element>,
  onRectChange: (rect: Rect) => void,
) {
  const viewport = instance.scrollElement;
  if (!viewport) return undefined;

  const emitRect = () => {
    onRectChange({
      width: viewport.offsetWidth,
      height: viewport.offsetHeight || DEFAULT_CONTAINER_HEIGHT,
    });
  };

  emitRect();

  const ResizeObserverImpl = instance.targetWindow?.ResizeObserver;
  if (!ResizeObserverImpl) return undefined;

  const observer = new ResizeObserverImpl(emitRect);
  observer.observe(viewport);
  return () => observer.disconnect();
}

export function DenseDataTable<T>({
  columns,
  rows,
  getRowKey,
  selectedRowKey,
  onRowClick,
  onRowDoubleClick,
  renderRowContextMenu,
  emptyTitle: emptyTitleProp,
  emptyDescription: emptyDescriptionProp,
  emptyAction,
  sortKey,
  sortDirection,
  onSort,
  onReachEnd,
  loadContextKey,
  loadStateKey,
  hasMore = false,
  loadingMore = false,
  loadMoreFailed = false,
  loadMoreErrorText: loadMoreErrorTextProp,
  retryLoadMoreLabel: retryLoadMoreLabelProp,
  onRetryLoadMore,
  initialLoadFailed = false,
  initialLoadErrorText: initialLoadErrorTextProp,
  retryInitialLoadLabel: retryInitialLoadLabelProp,
  onRetryInitialLoad,
  horizontalScroll = false,
  minColumnWidth = 140,
  filterable = false,
}: DenseDataTableProps<T>) {
  const { t } = useTranslation();
  // Defaults live in i18n so the shared primitive never carries hardcoded copy.
  const emptyTitle = emptyTitleProp ?? t('denseTable.emptyTitle');
  const emptyDescription = emptyDescriptionProp ?? t('denseTable.emptyDescription');
  const loadMoreErrorText = loadMoreErrorTextProp ?? t('denseTable.loadMoreError');
  const retryLoadMoreLabel = retryLoadMoreLabelProp ?? t('denseTable.retry');
  const initialLoadErrorText = initialLoadErrorTextProp ?? t('denseTable.initialLoadError');
  const retryInitialLoadLabel = retryInitialLoadLabelProp ?? t('denseTable.retry');
  const containerRef = useRef<HTMLDivElement>(null);
  const [focusedIndex, setFocusedIndex] = useState(-1);
  const requestedRowCountRef = useRef<number | undefined>(undefined);
  const automaticRetryKeyRef = useRef<string | undefined>(undefined);
  const automaticRetryTimerRef = useRef<number | undefined>(undefined);
  const retryInFlightRef = useRef(false);
  const wasLoadingMoreRef = useRef(false);
  const filter = useDenseTableFilter({
    columns,
    rows,
    enabled: filterable,
    resetKey: loadContextKey,
  });
  const visibleRows = filter.visibleRows;
  // Callers usually pass an inline arrow for getRowKey, which would give getItemKey a new
  // identity every render; the virtualizer compares that dependency by identity, so every
  // scroll frame rebuilt the measurement array for all loaded rows. Read through a ref and
  // let only the visible row set invalidate the key function.
  const getRowKeyRef = useRef(getRowKey);
  getRowKeyRef.current = getRowKey;
  const getItemKey = useCallback((index: number) => getRowKeyRef.current(visibleRows[index]), [visibleRows]);
  const rowVirtualizer = useVirtualizer({
    count: visibleRows.length,
    getScrollElement: () => containerRef.current,
    estimateSize: () => DENSE_TABLE_ROW_HEIGHT,
    overscan: OVERSCAN_ROWS,
    getItemKey,
    initialRect: { width: 0, height: DEFAULT_CONTAINER_HEIGHT },
    observeElementRect: observeTableViewport,
  });

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
    requestedRowCountRef.current = undefined;
    automaticRetryKeyRef.current = undefined;
    retryInFlightRef.current = false;
    if (automaticRetryTimerRef.current !== undefined) {
      window.clearTimeout(automaticRetryTimerRef.current);
      automaticRetryTimerRef.current = undefined;
    }
    if (containerRef.current) {
      containerRef.current.scrollTop = 0;
    }
    rowVirtualizer.scrollToOffset(0);
  }, [loadContextKey, rowVirtualizer]);

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
    const remaining = container.scrollHeight - container.scrollTop - container.clientHeight;
    if (remaining <= DENSE_TABLE_ROW_HEIGHT * OVERSCAN_ROWS) {
      requestMore();
    }
  }, [requestMore]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container || !hasMore || loadingMore || loadMoreFailed || !onReachEnd) return;
    if (container.clientHeight <= 0) return;

    const remaining = container.scrollHeight - container.scrollTop - container.clientHeight;
    if (remaining > DENSE_TABLE_ROW_HEIGHT * OVERSCAN_ROWS) return;
    requestMore();
  }, [
    hasMore,
    loadContextKey,
    loadMoreFailed,
    loadingMore,
    onReachEnd,
    requestMore,
    rows.length,
  ]);

  const virtualRows = rowVirtualizer.getVirtualItems();
  const topSpacerHeight = virtualRows[0]?.start ?? 0;
  const bottomSpacerHeight = Math.max(
    0,
    rowVirtualizer.getTotalSize() - (virtualRows.at(-1)?.end ?? 0),
  );
  const tableStyle = horizontalScroll
    ? {
        width: `${Math.max(columns.length, 1) * minColumnWidth}px`,
        minWidth: '100%',
      }
    : undefined;

  useEffect(() => { setFocusedIndex(-1); }, [loadContextKey]);

  /** Focused-row copy: only columns that expose a text extractor contribute a cell. */
  const copyFocusedRow = useCallback((row: T) => {
    const line = columns.filter((column) => column.text).map((column) => column.text?.(row) ?? '').join('\t');
    const clipboard = typeof navigator === 'undefined' ? undefined : navigator.clipboard;
    if (!line || !clipboard?.writeText) return false;
    void clipboard.writeText(line);
    toast(t('denseTable.copied'));
    return true;
  }, [columns, t]);

  const handleKeyDown = useCallback((event: KeyboardEvent<HTMLDivElement>) => {
    const lastIndex = visibleRows.length - 1;
    if (lastIndex < 0) return;
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp' || event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      setFocusedIndex((current) => {
        if (event.key === 'Home') return 0;
        if (event.key === 'End') return lastIndex;
        const step = event.key === 'ArrowDown' ? 1 : -1;
        const base = current < 0 ? (step > 0 ? -1 : lastIndex + 1) : current;
        return Math.min(Math.max(base + step, 0), lastIndex);
      });
      return;
    }
    if (focusedIndex < 0) return;
    const row = visibleRows[focusedIndex];
    if (!row) return;
    if (event.key === 'Enter') {
      event.preventDefault();
      onRowClick?.(row);
      return;
    }
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'c' && copyFocusedRow(row)) {
      event.preventDefault();
    }
  }, [copyFocusedRow, focusedIndex, onRowClick, visibleRows]);

  useEffect(() => {
    if (focusedIndex < 0) return;
    rowVirtualizer.scrollToIndex(focusedIndex, { align: 'auto' });
  }, [focusedIndex, rowVirtualizer]);

  const table = (
    <ScrollArea
      className="min-h-0 flex-1 overflow-hidden bg-transparent font-mono text-[11px]"
      viewportRef={containerRef}
      viewportClassName={horizontalScroll ? 'overflow-x-auto scrollbar-none' : 'overflow-x-hidden'}
      viewportProps={{
        onScroll: handleScroll,
        onKeyDown: handleKeyDown,
        tabIndex: 0,
        'aria-label': t('denseTable.gridLabel'),
        style: { contain: 'strict' },
      }}
      showHorizontalScrollbar={horizontalScroll}
    >
      <Table
        className="text-[11px]"
        // A clipped intermediary becomes sticky's nearest scroll container
        // even though it never scrolls. Keep the wrapper visible so the
        // Radix viewport above remains the header's scroll container.
        containerClassName="overflow-visible"
        style={tableStyle}
      >
        <TableHeader className="sticky top-0 z-10 bg-forensics-panel">
          <TableRow className="border-b border-forensics-border hover:bg-forensics-panel">
            {columns.map((column) => (
              <TableHead
                key={column.key}
                className={`group h-7 overflow-hidden border-r border-forensics-border px-2 text-[11px] font-light tracking-wide text-forensics-text-tertiary last:border-r-0 ${
                  column.sortable ? 'cursor-pointer select-none hover:bg-forensics-hover' : ''
                } ${column.className ?? ''}`}
                onClick={() => column.sortable && handleSort(column.sortKey ?? column.key)}
              >
                <div className="flex min-w-0 items-center gap-1">
                  <span
                    className="min-w-0 flex-1 truncate"
                    title={typeof column.title === 'string' ? column.title : undefined}
                  >
                    {column.title}
                  </span>
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
          <DenseTableStatusRows
            columnCount={columns.length}
            loadedCount={rows.length}
            visibleCount={visibleRows.length}
            initialLoadFailed={initialLoadFailed}
            initialLoadErrorText={initialLoadErrorText}
            retryInitialLoadLabel={retryInitialLoadLabel}
            onRetryInitialLoad={onRetryInitialLoad}
            emptyTitle={emptyTitle}
            emptyDescription={emptyDescription}
            emptyAction={emptyAction}
            noMatchText={t('denseTable.noMatch')}
          />
          {visibleRows.length > 0 && topSpacerHeight > 0 ? (
            <TableRow aria-hidden="true" className="hover:bg-transparent">
              <TableCell
                colSpan={columns.length}
                className="border-r-0 p-0"
                style={{ height: topSpacerHeight }}
              />
            </TableRow>
          ) : null}
          {virtualRows.map((virtualRow) => {
            const row = visibleRows[virtualRow.index];
            if (!row) return null;
            const key = getRowKey(row);
            const selected = key === selectedRowKey;
            return (
              <DenseDataTableRow
                key={key}
                row={row}
                columns={columns}
                selected={selected}
                focused={virtualRow.index === focusedIndex}
                onRowClick={onRowClick}
                onRowDoubleClick={onRowDoubleClick}
                renderRowContextMenu={renderRowContextMenu}
              />
            );
          })}
          {visibleRows.length > 0 && bottomSpacerHeight > 0 ? (
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
                {t('denseTable.loadingMore')}
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
                  <Button aria-label={retryLoadMoreLabel}
                    type="button"
                    variant="forensicsOutline"
                    size="compact"
                    onClick={retryMore}
                  ><RefreshCw size={15} /></Button>
                </div>
              </TableCell>
            </TableRow>
          ) : null}
        </TableBody>
      </Table>
    </ScrollArea>
  );

  if (!filterable) return table;

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <DenseTableFilterBar
        keyword={filter.keywordInput}
        onKeywordChange={filter.setKeywordInput}
        selects={filter.selects}
        onSelectChange={filter.setSelectValue}
        filterActive={filter.filterActive}
        filteredCount={visibleRows.length}
        loadedCount={rows.length}
      />
      {table}
    </div>
  );
}
