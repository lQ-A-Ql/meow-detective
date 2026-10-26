import { useCallback, useEffect, useMemo, useRef } from 'react';
import { useFileNavigationStore } from '@/stores/file-navigation-store';
import { useSelectionStore } from '@/stores/selection-store';
import type { FileSortDirection, FileSortKey } from '@/lib/file-sort';
import { useFileJumpContext } from '@/features/files/hooks';
import { isDataSourceTreeNodeId } from '../file-tree-utils';

interface FileBrowserLocationOptions {
  caseId?: string;
  showHidden: boolean;
  pageLimit: number;
  sortKey: FileSortKey;
  sortDirection: FileSortDirection;
}

export function useFileBrowserLocation({ caseId, showHidden, pageLimit, sortKey, sortDirection }: FileBrowserLocationOptions) {
  const selectedDirectoryId = useSelectionStore((state) => state.selectedDirectoryId);
  const selectedFileId = useSelectionStore((state) => state.selectedFileId);
  const request = useFileNavigationStore((state) => state.request);
  const pending = useFileNavigationStore((state) => state.pending);
  const owner = useRef({ caseId, initialized: false });
  if (owner.current.caseId !== caseId) owner.current = { caseId, initialized: false };

  useEffect(() => {
    if (!caseId || owner.current.caseId !== caseId || owner.current.initialized) return;
    owner.current.initialized = true;
    const fileId = useSelectionStore.getState().selectedFileId;
    if (fileId && !useFileNavigationStore.getState().request) {
      useFileNavigationStore.getState().begin(caseId, fileId, 'restore');
    }
  }, [caseId, selectedFileId]);

  const restoring = Boolean(request && caseId && request.caseId === caseId && request.origin === 'restore' && !pending);
  const restoration = useFileJumpContext(restoring ? request?.fileId : undefined,
    true, pageLimit, sortKey, sortDirection, caseId);
  useEffect(() => {
    if (restoring && restoration.data && request) {
      useFileNavigationStore.getState().resolve(request, restoration.data);
    }
  }, [request, restoration.data, restoring]);

  const directory = useFileJumpContext(
    isDataSourceTreeNodeId(selectedDirectoryId) ? undefined : selectedDirectoryId,
    showHidden, pageLimit, sortKey, sortDirection, caseId,
  );
  const directoryContext = directory.data?.directory.id === selectedDirectoryId ? directory.data : undefined;
  const pendingJump = pending?.request.caseId === caseId ? pending : undefined;
  const context = pendingJump?.context ?? directoryContext;
  const revealDirectoryIds = useMemo(() => context
    ? [...new Set([...context.ancestorDirectoryIds, context.directory.id])]
    : undefined, [context]);

  const cancelNavigation = useCallback(() => {
    owner.current.initialized = true;
    useFileNavigationStore.getState().cancel();
  }, []);
  const selectDirectory = useCallback((id?: string) => {
    cancelNavigation();
    useSelectionStore.setState({ selectedDirectoryId: id, selectedFileId: undefined });
  }, [cancelNavigation]);
  const selectFile = useCallback((id?: string) => {
    cancelNavigation();
    useSelectionStore.getState().setSelectedFileId(id);
  }, [cancelNavigation]);

  return {
    pendingJump,
    directoryContext,
    revealDirectoryIds,
    pathNodes: context?.ancestorNodes,
    requestPending: Boolean(caseId && request?.caseId === caseId),
    deferInitialSelection: Boolean(selectedFileId || (caseId && request?.caseId === caseId)),
    selectDirectory,
    selectFile,
    cancelNavigation,
    directoryError: directory.error ?? restoration.error,
    retryDirectory: () => restoring ? restoration.refetch() : directory.refetch(),
  };
}
