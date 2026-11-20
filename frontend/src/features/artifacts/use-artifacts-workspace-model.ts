import { useCallback, useEffect, useMemo } from 'react';
import {
  useArtifactById,
  useArtifactFamilies,
  useArtifactFamilyCounts,
  useInfiniteArtifactRows,
} from '@/features/artifacts/hooks';
import { useArtifactsSelectionModel } from '@/features/artifacts/use-artifacts-page-model';
import { useCapabilityMatrix } from '@/features/capability/hooks';
import type { ArtifactRow, SourceCapability } from '@/types/models';

/**
 * Pick the case-wide artifacts advisory row: an in-flight (`running`/`queued`)
 * row wins over a `partial` row, which in turn wins over a `deferred` row. A
 * `ready` row never surfaces here, so partial results are never presented as
 * complete. Rows for other capabilities are ignored.
 */
export function pickArtifactsCapabilityAdvisory(rows: SourceCapability[]): SourceCapability | undefined {
  const artifacts = rows.filter((row) => row.capability === 'artifacts');
  return (
    artifacts.find((row) => row.state === 'running' || row.state === 'queued') ??
    artifacts.find((row) => row.state === 'partial') ??
    artifacts.find((row) => row.state === 'deferred')
  );
}

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
  // Case-wide advisory: surface the first non-ready artifacts row so partial
  // results are never presented as complete; running/deferred stay informational.
  const { data: capabilityMatrix } = useCapabilityMatrix();
  const artifactsCapability = useMemo<SourceCapability | undefined>(
    () => pickArtifactsCapabilityAdvisory(capabilityMatrix?.sources ?? []),
    [capabilityMatrix],
  );
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
    artifactsCapabilityState: artifactsCapability?.state,
    artifactsCapabilityDetail: artifactsCapability?.detail,
  };
}

export type ArtifactsWorkspaceModel = ReturnType<typeof useArtifactsWorkspaceModel>;
