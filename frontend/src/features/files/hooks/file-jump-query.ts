import { getFileJumpContext } from '@/lib/api/files';
import type { FileSortDirection, FileSortKey } from '@/lib/file-sort';

export function fileJumpQuery(caseId: string | undefined, fileId: string | undefined, showHidden = false,
  pageLimit = 500, sortKey: FileSortKey = 'name', sortDirection: FileSortDirection = 'asc') {
  return {
    queryKey: ['files', 'jump-context', caseId ?? null, fileId ?? null, showHidden, pageLimit, sortKey, sortDirection],
    queryFn: async ({ signal }: { signal: AbortSignal }) => {
      const result = await getFileJumpContext(fileId!, { showHidden, pageLimit, sortKey, sortDirection });
      signal.throwIfAborted();
      return result;
    },
    enabled: Boolean(caseId && fileId),
    retry: false,
    staleTime: 5_000,
  };
}
