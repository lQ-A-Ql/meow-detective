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
  pagination: Pick<ReturnType<typeof useFilePagination>,
    'rowsOffset' | 'setRowsOffset' | 'rows' | 'rowsReady' | 'goToPreviousRows' | 'goToNextRows' | 'handleSort'>;
}

export function useFileBrowserCoordinator({ caseId, selectedDirectoryId, selectedFileId,
  showHidden, onShowHidden, location, tree, pagination }: FileBrowserCoordinatorOptions) {
  const setSelectedDirectoryId = location.selectDirectory;
  const setSelectedFileId = location.selectFile;

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
    tree.setExpandedDirectoryIds((current) => {
      const next = [...new Set([...current, ...directoryExpansionIds(context, true)])];
      return next.length === current.length ? current : next;
    });
    const offset = context.target.entryType === 'file' ? context.rowOffset : 0;
    if (pagination.rowsOffset !== offset) {
      pagination.setRowsOffset(offset);
      return;
    }
    useFileNavigationStore.getState().consume(pending);
  }, [location.pendingJump, onShowHidden, pagination.rowsOffset, pagination.setRowsOffset, selectedDirectoryId, selectedFileId,
    showHidden, tree.setExpandedDirectoryIds]);

  useEffect(() => {
    const context = location.directoryContext;
    if (!context || location.pendingJump) return;
    const visibility = hiddenDirectoryDestination(context);
    if (!showHidden && visibility.hidden) {
      setSelectedDirectoryId(visibility.directoryId);
      return;
    }
    tree.setExpandedDirectoryIds((current) => {
      const next = [...new Set([...current, ...directoryExpansionIds(context)])];
      return next.length === current.length ? current : next;
    });
  }, [location.directoryContext, location.pendingJump, setSelectedDirectoryId, showHidden,
    tree.setExpandedDirectoryIds]);

  useEffect(() => {
    if (selectedFileId && pagination.rowsReady && !location.pendingJump
      && !pagination.rows?.some((row) => row.id === selectedFileId)
      && useFileNavigationStore.getState().request?.caseId !== caseId) {
      setSelectedFileId(undefined);
    }
  }, [caseId, location.pendingJump, location.requestPending, pagination.rows, pagination.rowsReady,
    selectedFileId, setSelectedFileId]);

  const setShowHidden = useCallback((value: boolean) => {
    setSelectedFileId(undefined);
    onShowHidden(value);
  }, [onShowHidden, setSelectedFileId]);
  const handleSort = useCallback((key: string) => {
    setSelectedFileId(undefined);
    pagination.handleSort(key);
  }, [pagination.handleSort, setSelectedFileId]);
  const goToPreviousRows = useCallback(() => {
    setSelectedFileId(undefined);
    pagination.goToPreviousRows();
  }, [pagination.goToPreviousRows, setSelectedFileId]);
  const goToNextRows = useCallback(() => {
    setSelectedFileId(undefined);
    pagination.goToNextRows();
  }, [pagination.goToNextRows, setSelectedFileId]);
  const activateEntry = useCallback((row: FileEntryRow) => {
    if (row.entryType === 'directory') {
      setSelectedDirectoryId(row.id);
      tree.setExpandedDirectoryIds((current) => current.includes(row.id) ? current : [...current, row.id]);
    } else {
      setSelectedFileId(row.id);
    }
  }, [setSelectedDirectoryId, setSelectedFileId, tree.setExpandedDirectoryIds]);

  return { setShowHidden, handleSort, goToPreviousRows, goToNextRows, activateEntry };
}
