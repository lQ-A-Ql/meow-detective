import { useQuery } from '@tanstack/react-query';
import { openFileHandle } from '@/lib/api/files';
import { adoptPreviewHandle, usePreviewHandleOwner } from '../previewHandleOwner';
import { PREVIEW_STALE_TIME } from './preview-constants';

export function useFileHandle(fileId?: string, enabled = true, caseId?: string) {
  const owner = usePreviewHandleOwner(`${fileId ?? ''}:handle`);
  const query = useQuery({
    queryKey: ['files', 'handle', caseId ?? null, fileId ?? null],
    enabled: Boolean(fileId) && enabled,
    retry: false,
    staleTime: PREVIEW_STALE_TIME,
    gcTime: 0,
    queryFn: async ({ signal }) => {
      const handle = await openFileHandle(fileId!);
      await adoptPreviewHandle(owner, handle.handleId, signal);
      return handle;
    },
  });
  return query;
}
