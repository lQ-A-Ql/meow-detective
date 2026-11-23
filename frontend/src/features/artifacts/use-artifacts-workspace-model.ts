import { useCallback, useEffect, useMemo } from 'react';
import {
  useArtifactById,
  useArtifactFamilies,
  useArtifactFamilyCounts,
  useInfiniteArtifactRows,
} from '@/features/artifacts/hooks';
import { useArtifactsSelectionModel } from '@/features/artifacts/use-artifacts-page-model';
import { useCapabilityMatrix, useRetrySourceCapability } from '@/features/capability/hooks';
import { artifactAdvisoryPresentation, pickArtifactsCapabilityAdvisory } from './capability-advisory';
import { errorMessage } from '@/lib/errors';
import type { ArtifactRow, SourceCapability } from '@/types/models';

export { pickArtifactsCapabilityAdvisory } from './capability-advisory';

/** Owns artifact queries, persisted selection state, and cross-workspace navigation. */
export function useArtifactsWorkspaceModel() {
  const {
    openArtifactSource,
    openArtifactTimeline,
    selectedArtifactFamily,
    selectedArtifactId,
    setSelectedArtifactFamily,
    setSelectedArtifactId,
  } = useArtifactsSelectionModel();
  const artifactFamilies = useArtifactFamilies();
  const artifactFamilyCounts = useArtifactFamilyCounts();
  const artifactRowsQuery = useInfiniteArtifactRows(selectedArtifactFamily);
  const rows = useMemo(
    () => (artifactRowsQuery.data?.pages ?? []).flatMap((page) => page?.items ?? []),
    [artifactRowsQuery.data],
  );
  const totalRows = artifactRowsQuery.data?.pages[0]?.total ?? 0;
  const selectedArtifactQuery = useArtifactById(selectedArtifactId);

  useEffect(() => {
    const selectedArtifact = selectedArtifactQuery.data;
    if (selectedArtifact?.artifactType && selectedArtifact.artifactType !== selectedArtifactFamily) {
      setSelectedArtifactFamily(selectedArtifact.artifactType);
    }
  }, [selectedArtifactFamily, selectedArtifactQuery.data, setSelectedArtifactFamily]);

  const tableRows = useMemo(() => {
    if (!selectedArtifactQuery.data || rows.some((row) => row.id === selectedArtifactQuery.data?.id)) {
      return rows;
    }
    return [selectedArtifactQuery.data, ...rows];
  }, [rows, selectedArtifactQuery.data]);
  const selectedArtifact =
    selectedArtifactQuery.data ?? tableRows.find((row) => row?.id === selectedArtifactId) ?? tableRows[0];
  const { data: capabilityMatrix } = useCapabilityMatrix();
  const capabilityRetry = useRetrySourceCapability();
  const artifactsCapability = useMemo<SourceCapability | undefined>(
    () => pickArtifactsCapabilityAdvisory(capabilityMatrix?.sources ?? []),
    [capabilityMatrix],
  );
  const advisory = artifactsCapability ? artifactAdvisoryPresentation(artifactsCapability) : undefined;
  const retryArtifactsCapability = useCallback(() => {
    if (!artifactsCapability || !advisory?.retryable || capabilityRetry.isPending) return;
    capabilityRetry.mutate({ dataSourceId: artifactsCapability.dataSourceId, capability: 'artifacts' });
  }, [artifactsCapability, advisory?.retryable, capabilityRetry]);
  const systemdStateCounts = useMemo(() => {
    if (selectedArtifactFamily !== 'LinuxSystemConfig') return undefined;
    return tableRows.reduce<Record<string, number>>((counts, row) => {
      if (row.attrs.configKind === 'systemdServiceUnit') {
        const state = String(row.attrs.state ?? 'unknown');
        counts[state] = (counts[state] ?? 0) + 1;
      }
      return counts;
    }, {});
  }, [selectedArtifactFamily, tableRows]);
  const families = useMemo(() => {
    const counts = new Map(artifactFamilyCounts.data?.map((entry) => [entry.family, entry.count]));
    return (artifactFamilies.data ?? []).map((family) => ({
      family,
      count: counts.get(family) ?? tableRows.length,
    }));
  }, [artifactFamilies.data, artifactFamilyCounts.data, tableRows.length]);
  const onArtifactRowClick = useCallback(
    (artifact: ArtifactRow) => setSelectedArtifactId(artifact.id),
    [setSelectedArtifactId],
  );
  const loadNextPage = useCallback(() => {
    void artifactRowsQuery.fetchNextPage();
  }, [artifactRowsQuery]);
  const retry = useCallback(() => {
    void artifactRowsQuery.refetch();
  }, [artifactRowsQuery]);

  return {
    families,
    hasMore: artifactRowsQuery.hasNextPage,
    initialLoadFailed: artifactRowsQuery.isError && rows.length === 0,
    loadContextKey: selectedArtifactFamily,
    loadMoreFailed: artifactRowsQuery.isFetchNextPageError,
    loadNextPage,
    loadStateKey: artifactRowsQuery.dataUpdatedAt,
    loadingMore: artifactRowsQuery.isFetchingNextPage,
    onArtifactRowClick,
    openArtifactSource,
    openArtifactTimeline,
    retry,
    selectedArtifact,
    selectedArtifactFamily,
    selectArtifactFamily: setSelectedArtifactFamily,
    tableRows,
    totalRows,
    artifactsAdvisory: advisory,
    retryArtifactsCapability,
    retryingArtifacts: capabilityRetry.isPending,
    artifactsRetryError:
      capabilityRetry.error && capabilityRetry.variables?.dataSourceId === artifactsCapability?.dataSourceId
        ? errorMessage(capabilityRetry.error)
        : undefined,
    systemdStateCounts,
  };
}

export type ArtifactsWorkspaceModel = ReturnType<typeof useArtifactsWorkspaceModel>;
