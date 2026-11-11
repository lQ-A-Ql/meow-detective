import { useMutation, useQueryClient } from '@tanstack/react-query';
import { importDataSource } from '@/lib/api/files';
import type { ImportDataSourceRequest } from '@/types/models';
import { expectJobsSnapshotActivity } from '@/features/jobs/hooks';
import { useCurrentCase } from '@/features/case/hooks';
import { invalidateImportProjectionQueries } from '@/features/cache-invalidation';

export function useImportDataSource() {
  const qc = useQueryClient();
  const currentCase = useCurrentCase();
  const jobsQueryKey = ['jobs', 'snapshot', currentCase.data?.id ?? null];
  return useMutation({
    mutationFn: (request: ImportDataSourceRequest) => importDataSource(request),
    onMutate: () => {
      expectJobsSnapshotActivity(qc.getQueryData(jobsQueryKey));
      qc.invalidateQueries({ queryKey: jobsQueryKey });
    },
    onSuccess: () => {
      expectJobsSnapshotActivity();
      invalidateImportProjectionQueries(qc);
      qc.invalidateQueries({ queryKey: jobsQueryKey });
    },
  });
}
