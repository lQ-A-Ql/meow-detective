import { useMemo } from 'react';
import { useQuery } from '@tanstack/react-query';
import { getSourceCapabilities } from '@/lib/api/capability';
import { useCurrentCase } from '@/features/case/hooks';
import { CAPABILITY_MATRIX_QUERY_KEY } from '@/features/cache-invalidation';
import type { CapabilityKind, SourceCapability } from '@/types/models';

export { CAPABILITY_MATRIX_QUERY_KEY };

/**
 * Case-wide capability matrix. `staleTime` keeps the matrix stable between
 * progress frames; the application event subscription invalidates it on state
 * transitions so gating reacts without polling.
 */
export function useCapabilityMatrix() {
  const currentCase = useCurrentCase();
  const caseId = currentCase.data?.id;

  const query = useQuery({
    queryKey: [...CAPABILITY_MATRIX_QUERY_KEY, caseId ?? null],
    queryFn: getSourceCapabilities,
    enabled: currentCase.isSuccess && Boolean(caseId),
    staleTime: 15_000,
    retry: false,
  });

  return query;
}

/** Index rows of one capability kind by dataSourceId for O(1) gating lookups. */
export function useCapabilityMap(capability: CapabilityKind): ReadonlyMap<string, SourceCapability> {
  const { data: matrix } = useCapabilityMatrix();
  return useMemo(() => {
    const map = new Map<string, SourceCapability>();
    for (const row of matrix?.sources ?? []) {
      if (row.capability === capability) {
        map.set(row.dataSourceId, row);
      }
    }
    return map;
  }, [matrix, capability]);
}

/** Read one source's row for a capability kind; undefined when absent. */
export function useSourceCapability(
  dataSourceId: string | undefined,
  capability: CapabilityKind,
): SourceCapability | undefined {
  const { data: matrix } = useCapabilityMatrix();
  return useMemo(() => {
    if (!dataSourceId) {
      return undefined;
    }
    return matrix?.sources.find((row) => row.dataSourceId === dataSourceId && row.capability === capability);
  }, [matrix, dataSourceId, capability]);
}
