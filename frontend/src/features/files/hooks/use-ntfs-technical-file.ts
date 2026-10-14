import { useQuery } from '@tanstack/react-query';
import { inspectNtfsFile } from '@/lib/api/files';
import { errorMessage, isApiErrorDto } from '@/lib/errors';
import type { NtfsTechnicalFile } from '@/types/files';
import { isSourceScopedMftId } from '../source-scoped-file-id';
import { ntfsInspectorRuns } from '../ntfs-inspector-runs';

export function useNtfsTechnicalFile(caseId?: string, fileId?: string, isFile = false) {
  const eligible = Boolean(caseId && isFile && isSourceScopedMftId(fileId));
  const query = useQuery<NtfsTechnicalFile>({
    queryKey: ['files', 'ntfs-technical', caseId, fileId],
    enabled: eligible,
    retry: false,
    staleTime: Infinity,
    queryFn: () => inspectNtfsFile(fileId!),
  });
  const technical = eligible && !query.isError ? query.data : undefined;
  return {
    eligible,
    technical,
    runs: ntfsInspectorRuns(technical),
    loading: eligible && query.isFetching,
    errorText: eligible && query.error != null ? errorMessage(query.error) : undefined,
    unsupported: eligible && isApiErrorDto(query.error) && query.error.category === 'unsupported',
    retry: () => { if (eligible) void query.refetch(); },
  };
}
