import { QueryClient } from '@tanstack/react-query';
import { useEffect, useState } from 'react';
import { subscribeToEvent } from '@/lib/events/subscribers';
import { subscribeToJobSnapshotEvents } from '@/features/jobs/job-snapshot-events';
import {
  PROJECTION_EVENT_TOPICS,
  invalidateCacheStatusQueries,
  invalidateEventProjectionQueries,
  invalidatePartialResultQueries,
} from '@/features/cache-invalidation';
import type { CapabilityStatusChangedPayload, IndexCacheStatus, PartialResult } from '@/types/models';

function subscribeToProjectionInvalidations(queryClient: QueryClient) {
  const unsubscribeJobs = subscribeToJobSnapshotEvents(queryClient);
  const unsubs = PROJECTION_EVENT_TOPICS.map((topic) =>
    subscribeToEvent<CapabilityStatusChangedPayload>(topic, (event) => {
      invalidateEventProjectionQueries(queryClient, topic, event.payload);
    }),
  );
  unsubs.push(
    subscribeToEvent<PartialResult>('import-partial-result', (event) => {
      invalidatePartialResultQueries(queryClient, event.payload);
    }),
  );
  unsubs.push(
    subscribeToEvent<IndexCacheStatus>('cache-index-status', (event) => {
      invalidateCacheStatusQueries(queryClient, event.payload);
    }),
  );
  return () => {
    unsubscribeJobs();
    unsubs.forEach((unsubscribe) => unsubscribe());
  };
}

export function useWorkbenchQueryClient() {
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: { staleTime: 30_000, refetchOnWindowFocus: false },
        },
      }),
  );
  useEffect(() => subscribeToProjectionInvalidations(queryClient), [queryClient]);
  return queryClient;
}
