/**
 * DenseTableStatusRows - DenseDataTable 表体内的状态行（错误/空态/筛选无命中）。
 */

import { TableCell, TableRow } from '@/app/components/ui/table';
import { Button } from '@/app/components/ui/button';

interface DenseTableStatusRowsProps {
  columnCount: number;
  /** 已加载行数（未过滤）。 */
  loadedCount: number;
  /** 过滤后可见行数；未启用过滤时等于 loadedCount。 */
  visibleCount: number;
  initialLoadFailed: boolean;
  initialLoadErrorText: string;
  retryInitialLoadLabel: string;
  onRetryInitialLoad?: () => void;
  emptyTitle: string;
  emptyDescription: string;
  noMatchText: string;
}

export function DenseTableStatusRows({
  columnCount,
  loadedCount,
  visibleCount,
  initialLoadFailed,
  initialLoadErrorText,
  retryInitialLoadLabel,
  onRetryInitialLoad,
  emptyTitle,
  emptyDescription,
  noMatchText,
}: DenseTableStatusRowsProps) {
  if (loadedCount === 0 && initialLoadFailed) {
    return (
      <TableRow aria-live="polite" className="hover:bg-transparent">
        <TableCell colSpan={columnCount} className="px-4 py-8">
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
    );
  }
  if (loadedCount === 0) {
    return (
      <TableRow className="hover:bg-transparent">
        <TableCell colSpan={columnCount} className="px-4 py-8">
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
    );
  }
  if (visibleCount === 0) {
    return (
      <TableRow className="hover:bg-transparent">
        <TableCell
          colSpan={columnCount}
          className="px-4 py-8 text-center text-[11px] text-forensics-muted"
        >
          {noMatchText}
        </TableCell>
      </TableRow>
    );
  }
  return null;
}
