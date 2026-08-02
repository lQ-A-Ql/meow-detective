import { useCallback, useMemo } from 'react';
import { useCaseOverviewSnapshot } from '@/features/analysis/hooks';
import { useCurrentCase } from '@/features/case/hooks';
import { useGraphSnapshot } from '@/features/graph/hooks';

/** Owns V3 dashboard queries and refresh orchestration. */
export function useV3DashboardModel() {
  const currentCase = useCurrentCase();
  const graph = useGraphSnapshot(currentCase.data?.id ?? '');
  const overview = useCaseOverviewSnapshot();
  const refresh = useCallback(async () => {
    await Promise.all([
      graph.refetch(),
      overview.refetch(),
    ]);
  }, [graph, overview]);
  const loading = currentCase.isLoading
    || graph.isLoading
    || overview.isLoading;
  const error = useMemo(
    () => currentCase.error
      ?? graph.error
      ?? overview.error,
    [currentCase.error, graph.error, overview.error],
  );

  return {
    currentCaseIsSuccess: currentCase.isSuccess,
    error,
    graph,
    hasCase: Boolean(currentCase.data),
    loading,
    overview,
    refresh,
  };
}

export type V3DashboardModel = ReturnType<typeof useV3DashboardModel>;
