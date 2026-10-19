import { useEffect } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { useEvidenceTask } from '@/features/jobs/hooks/use-evidence-task';
import { getContentIndexCoverage } from '@/lib/api/evidence-tasks';
export function useContentIndexModel(caseId?: string, sourceId?: string) {
  const task = useEvidenceTask(caseId, `content:${sourceId}`);
  const cache = useQueryClient();
  const coverage = useQuery({ queryKey: ['content-coverage', caseId, sourceId], enabled: !!caseId && !!sourceId,
    retry: false, staleTime: 15_000, refetchOnWindowFocus: false, queryFn: () => getContentIndexCoverage(sourceId!) });
  useEffect(() => {
    if (task.task?.state === 'completed') {
      void cache.invalidateQueries({ queryKey: ['content-coverage', caseId, sourceId] });
      void cache.resetQueries({ queryKey: ['search', caseId] });
    }
  }, [task.task?.state, task.task?.taskId, cache, caseId, sourceId]);
  return { ...task, coverage: coverage.data, coverageError: coverage.error, coverageLoading: coverage.isFetching,
    refresh: () => { void coverage.refetch(); }, available: !!caseId && !!sourceId,
    build: () => { if (caseId && sourceId && !task.loading) task.start({ kind: 'contentIndex', request: { dataSourceId: sourceId } }); } };
}
