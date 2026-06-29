import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { PropsWithChildren, useEffect, useState } from 'react';
import { subscribeToEvent } from '@/lib/events/subscribers';
import {
  invalidateCacheStatusQueries,
  invalidateEventProjectionQueries,
  invalidatePartialResultQueries,
} from '@/features/cache-invalidation';
import type { EventTopic, IndexCacheStatus, PartialResult } from '@/types/models';

const projectionEventTopics: EventTopic[] = [
  'case-opened',
  'case-closed',
  'data-source-imported',
  'artifact-added',
  'timeline-updated',
  'search-index-progress',
];

function subscribeToProjectionInvalidations(queryClient: QueryClient) {
  const unsubs = projectionEventTopics.map((topic) =>
    subscribeToEvent(topic, () => {
      invalidateEventProjectionQueries(queryClient, topic);
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

  return () => unsubs.forEach((unsubscribe) => unsubscribe());
}

export function AppProviders({ children }: PropsWithChildren) {
  const [queryClient] = useState(
    () =>
      new QueryClient({
        defaultOptions: {
          queries: {
            staleTime: 30_000,
            refetchOnWindowFocus: false,
          },
        },
      }),
  );

  useEffect(() => subscribeToProjectionInvalidations(queryClient), [queryClient]);

  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}
