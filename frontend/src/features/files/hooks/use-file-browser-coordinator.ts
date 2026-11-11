import { useCallback, useEffect } from 'react';
import { useFileNavigationStore } from '@/stores/file-navigation-store';
import type { FileEntryRow } from '@/types/models';
import { directoryExpansionIds, hiddenDirectoryDestination } from '../file-browser-location';
import type { useFileBrowserLocation } from './use-file-browser-location';
import type { useFilePagination } from './use-file-pagination';
import type { useFileTree } from './use-file-tree';

interface FileBrowserCoordinatorOptions {
  caseId?: string;
  selectedDirectoryId?: string;
  selectedFileId?: string;
  showHidden: boolean;
  onShowHidden: (value: boolean) => void;
  location: ReturnType<typeof useFileBrowserLocation>;
  tree: Pick<ReturnType<typeof useFileTree>, 'setExpandedDirectoryIds'>;
  pagination: Pick<
    ReturnType<typeof useFilePagination>,
    'rowsOffset' | 'setRowsOffset' | 'rows' | 'rowsReady' | 'goToPreviousRows' | 'goToNextRows' | 'handleSort'
  >;
}

export function useFileBrowserCoordinator({
  caseId,
  selectedDirectoryId,
  selectedFileId,
  showHidden,
  onShowHidden,
  location,
  tree,
  pagination,
}: FileBrowserCoordinatorOptions) {
  const setSelectedDirectoryId = location.selectDirectory;
  const setSelectedFileId = location.selectFile;
  // `tree` and `pagination` are hook result objects rebuilt on every render. Each
  // member used below is either a `useCallback` function or a primitive, so depend
  // on the members rather than on the container object (which would invalidate the
  // effects on every render).
  const setExpandedDirectoryIds = tree.setExpandedDirectoryIds;
  const rowsOffset = pagination.rowsOffset;
  const setRowsOffset = pagination.setRowsOffset;
  const paginationHandleSort = pagination.handleSort;
  const paginationGoToPreviousRows = pagination.goToPreviousRows;
  const paginationGoToNextRows = pagination.goToNextRows;

  useEffect(() => {
    const pending = location.pendingJump;
    if (!pending) return;
    const context = pending.context;
    const fileId = context.target.entryType === 'file' ? context.target.id : undefined;
    if (selectedDirectoryId !== context.directory.id || selectedFileId !== fileId) {
      useFileNavigationStore.getState().consume(pending);
      return;
    }
    if (context.requiresShowHidden && !showHidden) {
      onShowHidden(true);
      return;
    }
    setExpandedDirectoryIds((current) => {
      const next = [...new Set([...current, ...directoryExpansionIds(context, true)])];
      return next.length === current.length ? current : next;
    });
    const offset = context.target.entryType === 'file' ? context.rowOffset : 0;
    if (rowsOffset !== offset) {
      setRowsOffset(offset);
      return;
    }
    useFileNavigationStore.getState().consume(pending);
  }, [
    location.pendingJump,
    onShowHidden,
    rowsOffset,
    selectedDirectoryId,
    selectedFileId,
    setExpandedDirectoryIds,
    setRowsOffset,
    showHidden,
  ]);

  useEffect(() => {
    const context = location.directoryContext;
    if (!context || location.pendingJump) return;
    const visibility = hiddenDirectoryDestination(context);
    if (!showHidden && visibility.hidden) {
      setSelectedDirectoryId(visibility.directoryId);
      return;
    }
    setExpandedDirectoryIds((current) => {
      const next = [...new Set([...current, ...directoryExpansionIds(context)])];
      return next.length === current.length ? current : next;
    });
  }, [location.directoryContext, location.pendingJump, setExpandedDirectoryIds, setSelectedDirectoryId, showHidden]);

  useEffect(() => {
    if (
      selectedFileId &&
      pagination.rowsReady &&
      !location.pendingJump &&
      !pagination.rows?.some((row) => row.id === selectedFileId) &&
      useFileNavigationStore.getState().request?.caseId !== caseId
    ) {
      setSelectedFileId(undefined);
    }
  }, [
    caseId,
    location.pendingJump,
    location.requestPending,
    pagination.rows,
    pagination.rowsReady,
    selectedFileId,
    setSelectedFileId,
  ]);

  const setShowHidden = useCallback(
    (value: boolean) => {
      setSelectedFileId(undefined);
      onShowHidden(value);
    },
    [onShowHidden, setSelectedFileId],
  );
  const handleSort = useCallback(
    (key: string) => {
      setSelectedFileId(undefined);
      paginationHandleSort(key);
    },
    [paginationHandleSort, setSelectedFileId],
  );
  const goToPreviousRows = useCallback(() => {
    setSelectedFileId(undefined);
    paginationGoToPreviousRows();
  }, [paginationGoToPreviousRows, setSelectedFileId]);
  const goToNextRows = useCallback(() => {
    setSelectedFileId(undefined);
    paginationGoToNextRows();
  }, [paginationGoToNextRows, setSelectedFileId]);
  const activateEntry = useCallback(
    (row: FileEntryRow) => {
      if (row.entryType === 'directory') {
        setSelectedDirectoryId(row.id);
        setExpandedDirectoryIds((current) => (current.includes(row.id) ? current : [...current, row.id]));
      } else {
        setSelectedFileId(row.id);
      }
    },
    [setExpandedDirectoryIds, setSelectedDirectoryId, setSelectedFileId],
  );

  return { setShowHidden, handleSort, goToPreviousRows, goToNextRows, activateEntry };
}
