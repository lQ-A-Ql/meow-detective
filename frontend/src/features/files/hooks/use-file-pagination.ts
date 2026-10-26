import { useCallback, useRef, useState, type SetStateAction } from 'react';
import { useFileRowsPage } from '@/features/files/hooks';
import { useUiStore } from '@/stores/ui-store';

interface UseFilePaginationOptions {
  activeDirectoryId?: string;
  pageLimit: number;
  showHidden: boolean;
  caseId?: string;
}

export function useFilePagination({
  activeDirectoryId,
  pageLimit,
  showHidden,
  caseId,
}: UseFilePaginationOptions) {
  const fileSortKey = useUiStore((s) => s.fileSortKey);
  const fileSortDirection = useUiStore((s) => s.fileSortDirection);
  const setFileSortKey = useUiStore((s) => s.setFileSortKey);
  const toggleFileSortDirection = useUiStore((s) => s.toggleFileSortDirection);
  const scope = JSON.stringify([caseId, activeDirectoryId, pageLimit, showHidden, fileSortKey, fileSortDirection]);
  const owner = useRef({ scope });
  if (owner.current.scope !== scope) owner.current = { scope };
  const currentOwner = owner.current;
  const [page, setPage] = useState({ owner: currentOwner, offset: 0 });
  const rowsOffset = page.owner === currentOwner ? page.offset : 0;
  const setRowsOffset = useCallback((value: SetStateAction<number>) => {
    setPage((current) => {
      if (owner.current !== currentOwner) return current;
      const previous = current.owner === currentOwner ? current.offset : 0;
      return { owner: currentOwner, offset: typeof value === 'function' ? value(previous) : value };
    });
  }, [currentOwner]);

  const { data: rowsPage, isSuccess: rowsReady, error: rowsError, refetch: retryRows } = useFileRowsPage(
    activeDirectoryId,
    rowsOffset,
    pageLimit,
    showHidden,
    fileSortKey,
    fileSortDirection,
    caseId,
  );
  const rows = rowsPage?.rows;

  const handleSort = useCallback(
    (key: string) => {
      if (key === fileSortKey) {
        toggleFileSortDirection();
      } else {
        setFileSortKey(key as 'name' | 'size' | 'modifiedAt' | 'ext');
      }
    },
    [fileSortKey, setFileSortKey, toggleFileSortDirection]
  );

  const executableCount = rows?.filter((row) =>
    ['exe', 'dll'].includes(
      (row.ext ?? row.name.split('.').pop() ?? '').toLowerCase()
    )
  ).length ?? 0;
  const sortedRows = rows ?? [];
  const activeDirectoryPath = rows?.find(
    (row) => row.id === activeDirectoryId
  )?.path;

  const canGoToPreviousRows = rowsOffset > 0;
  const canGoToNextRows = Boolean(
    rowsPage && rowsPage.offset + rowsPage.limit < rowsPage.totalCount
  );
  const goToPreviousRows = useCallback(
    () => setRowsOffset((current) => Math.max(0, current - pageLimit)),
    [pageLimit, setRowsOffset]
  );
  const goToNextRows = useCallback(() => {
    if (!rowsPage) return;
    setRowsOffset((current) =>
      current + rowsPage.limit < rowsPage.totalCount
        ? current + pageLimit
        : current
    );
  }, [rowsPage, pageLimit, setRowsOffset]);

  return {
    rowsOffset,
    rowsContextKey: `${scope}:${rowsOffset}`,
    setRowsOffset,
    rowsPage,
    rowsReady,
    rowsError,
    retryRows,
    rows,
    sortedRows,
    executableCount,
    activeDirectoryPath,
    fileSortKey,
    fileSortDirection,
    handleSort,
    canGoToPreviousRows,
    canGoToNextRows,
    goToPreviousRows,
    goToNextRows,
  };
}
