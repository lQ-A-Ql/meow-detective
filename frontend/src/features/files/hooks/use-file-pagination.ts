import { useCallback, useEffect, useState } from 'react';
import { useFileRowsPage } from '@/features/files/hooks';
import { useUiStore } from '@/stores/ui-store';

interface UseFilePaginationOptions {
  activeDirectoryId?: string;
  pageLimit: number;
  showHidden: boolean;
}

export function useFilePagination({
  activeDirectoryId,
  pageLimit,
  showHidden,
}: UseFilePaginationOptions) {
  const fileSortKey = useUiStore((s) => s.fileSortKey);
  const fileSortDirection = useUiStore((s) => s.fileSortDirection);
  const setFileSortKey = useUiStore((s) => s.setFileSortKey);
  const toggleFileSortDirection = useUiStore((s) => s.toggleFileSortDirection);
  const [rowsOffset, setRowsOffset] = useState(0);

  const { data: rowsPage } = useFileRowsPage(
    activeDirectoryId,
    rowsOffset,
    pageLimit,
    showHidden,
    fileSortKey,
    fileSortDirection
  );
  const rows = rowsPage?.rows;

  useEffect(() => {
    setRowsOffset(0);
  }, [showHidden]);

  useEffect(() => {
    setRowsOffset(0);
  }, [activeDirectoryId, fileSortKey, fileSortDirection]);

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
    [pageLimit]
  );
  const goToNextRows = useCallback(() => {
    if (!rowsPage) return;
    setRowsOffset((current) =>
      current + rowsPage.limit < rowsPage.totalCount
        ? current + pageLimit
        : current
    );
  }, [rowsPage, pageLimit]);

  return {
    rowsOffset,
    setRowsOffset,
    rowsPage,
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
