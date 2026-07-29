import { useCallback, useMemo } from 'react';
import { useCorrelationSnapshot, useV3GovernanceSnapshot } from '@/features/analysis/hooks';
import { useArtifactFamilyCounts } from '@/features/artifacts/hooks';
import { useCurrentCase, useDataSources } from '@/features/case/hooks';
import { useGraphSnapshot } from '@/features/graph/hooks';
import { useTimelineEvents } from '@/features/timeline/hooks';

/** Owns V3 dashboard queries and refresh orchestration. */
export function useV3DashboardModel() {
  const currentCase = useCurrentCase();
  const dataSources = useDataSources();
  const graph = useGraphSnapshot(currentCase.data?.id ?? '');
  const timeline = useTimelineEvents({ limit: 1 });
  const artifactCounts = useArtifactFamilyCounts();
  const correlation = useCorrelationSnapshot();
  const governance = useV3GovernanceSnapshot();
  const refresh = useCallback(async () => {
    await Promise.all([
      graph.refetch(),
      timeline.refetch(),
      artifactCounts.refetch(),
      correlation.refetch(),
      governance.refetch(),
    ]);
  }, [artifactCounts, correlation, governance, graph, timeline]);
  const loading = currentCase.isLoading
    || graph.isLoading
    || timeline.isLoading
    || artifactCounts.isLoading
    || correlation.isLoading
    || governance.isLoading;
  const error = useMemo(
    () => currentCase.error
      ?? graph.error
      ?? timeline.error
      ?? artifactCounts.error
      ?? correlation.error
      ?? governance.error,
    [artifactCounts.error, correlation.error, currentCase.error, governance.error, graph.error, timeline.error],
  );

  return {
    artifactCounts,
    correlation,
    currentCaseIsSuccess: currentCase.isSuccess,
    dataSources: dataSources.data,
    error,
    governance,
    graph,
    hasCase: Boolean(currentCase.data),
    loading,
    refresh,
    timeline,
  };
}

export type V3DashboardModel = ReturnType<typeof useV3DashboardModel>;
