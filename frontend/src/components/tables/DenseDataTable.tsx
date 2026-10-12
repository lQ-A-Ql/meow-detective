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
import { ScrollArea } from '@/app/components/ui/scroll-area';
import { DenseDataTableRow } from './DenseDataTableRow';
import { DENSE_TABLE_ROW_HEIGHT } from './dense-table-metrics';
import { SortIndicator } from './SortIndicator';
import { DenseTableFilterBar } from './DenseTableFilterBar';
import { DenseTableStatusRows } from './DenseTableStatusRows';
import { useDenseTableKeyboard } from './useDenseTableKeyboard';
import { useDenseTableFilter } from './useDenseTableFilter';

import type { DenseDataTableProps } from './dense-table-types';
export type { DenseColumn } from './dense-table-types';

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

  const { focusedIndex, handleKeyDown } = useDenseTableKeyboard({ columns, visibleRows, onRowClick, loadContextKey, scrollToIndex: rowVirtualizer.scrollToIndex });

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
                className={`group h-7 overflow-hidden px-2 text-[11px] font-light text-forensics-text-tertiary ${
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
