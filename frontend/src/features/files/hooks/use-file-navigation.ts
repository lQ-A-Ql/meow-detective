import { useRef } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { useNavigate } from 'react-router';
import { useFileNavigationStore, type FileNavigationRequest } from '@/stores/file-navigation-store';
import { useUiStore } from '@/stores/ui-store';
import { errorMessage } from '@/lib/errors';
import { fileJumpQuery } from './file-jump-query';
import type { CaseSummary } from '@/types/models';

/** Resolve the source-owned target before changing the browser's selection. */
export function useFileNavigation(caseId?: string) {
  const cache = useQueryClient();
  const navigate = useNavigate();
  const activeRequest = useFileNavigationStore((state) => state.request);
  const owner = useRef({ caseId, attempt: 0 });
  if (owner.current.caseId !== caseId) owner.current = { caseId, attempt: 0 };
  const mutation = useMutation({
    mutationFn: async (operation: { owner: typeof owner.current; attempt: number; fileId: string; request: FileNavigationRequest }) => {
      const { fileSortKey, fileSortDirection } = useUiStore.getState();
      const context = await cache.fetchQuery(fileJumpQuery(operation.owner.caseId, operation.fileId,
        true, 500, fileSortKey, fileSortDirection));
      return { operation, context };
    },
    onSuccess: ({ operation, context }) => {
      if (operation.owner !== owner.current || operation.attempt !== owner.current.attempt) return;
      if (cache.getQueryData<CaseSummary | null>(['case', 'current'])?.id !== operation.owner.caseId) return;
      if (!useFileNavigationStore.getState().resolve(operation.request, context)) return;
      useUiStore.getState().setCurrentPage('files');
      navigate('/files');
    },
  });
  const current = mutation.variables?.owner === owner.current
    && mutation.variables.attempt === owner.current.attempt
    && mutation.variables.request === activeRequest;
  return {
    pending: current && mutation.isPending,
    errorText: current && mutation.error ? errorMessage(mutation.error) : undefined,
    jump(fileId: string) {
      if (!caseId) return;
      const request = useFileNavigationStore.getState().begin(caseId, fileId, 'external');
      mutation.mutate({ owner: owner.current, attempt: ++owner.current.attempt, fileId, request });
    },
  };
}
