import { useEffect, useMemo } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { getSourceCapabilities } from '@/lib/api/capability';
import { useCurrentCase } from '@/features/case/hooks';
import { subscribeToEvent } from '@/lib/events/subscribers';
import type { CapabilityKind, CapabilityState, SourceCapability } from '@/types/models';

export const CAPABILITY_MATRIX_QUERY_KEY = ['capability', 'matrix'] as const;

/** Payload of the `capability-status-changed` event (see transport contract). */
export interface CapabilityStatusChangedPayload {
  caseId?: string;
  dataSourceId?: string;
  capability?: CapabilityKind;
  state?: CapabilityState;
  revision?: number;
}

/**
 * Case-wide capability matrix. `staleTime` keeps the matrix stable between
 * progress frames; `capability-status-changed` invalidates it on every state
 * transition so gating reacts without polling.
 */
export function useCapabilityMatrix() {
  const currentCase = useCurrentCase();
  const caseId = currentCase.data?.id;
  const queryClient = useQueryClient();

  const query = useQuery({
    queryKey: [...CAPABILITY_MATRIX_QUERY_KEY, caseId ?? null],
    queryFn: getSourceCapabilities,
    enabled: currentCase.isSuccess && Boolean(caseId),
    staleTime: 15_000,
    retry: false,
  });

  useEffect(() => {
    if (!caseId) {
      return;
    }
    const unsubscribe = subscribeToEvent<CapabilityStatusChangedPayload>('capability-status-changed', (event) => {
      if (event.payload.caseId && event.payload.caseId !== caseId) {
        return;
      }
      void queryClient.invalidateQueries({ queryKey: [...CAPABILITY_MATRIX_QUERY_KEY, caseId] });
      // Minimal cursor protection: request DTOs carry no capability/revision
      // today (opaque cursors only), so on a state transition we drop cached
      // timeline/search pages instead of refetching through a stale cursor.
      // The typed CURSOR_INVALIDATED response remains the backend's job (L2).
      if (event.payload.capability === 'timeline') {
        void queryClient.invalidateQueries({ queryKey: ['timeline'] });
      } else if (event.payload.capability === 'search') {
        void queryClient.invalidateQueries({ queryKey: ['search'] });
      }
    });
    return () => {
      unsubscribe();
    };
  }, [caseId, queryClient]);

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
