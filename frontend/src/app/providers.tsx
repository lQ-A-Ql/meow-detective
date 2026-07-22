import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { PropsWithChildren, useEffect, useState } from 'react';
import { I18nextProvider } from 'react-i18next';
import i18n from '@/i18n';
import { subscribeToEvent } from '@/lib/events/subscribers';
import {
  PROJECTION_EVENT_TOPICS,
  invalidateCacheStatusQueries,
  invalidateEventProjectionQueries,
  invalidatePartialResultQueries,
} from '@/features/cache-invalidation';
import type { IndexCacheStatus, PartialResult } from '@/types/models';

function subscribeToProjectionInvalidations(queryClient: QueryClient) {
  const unsubs = PROJECTION_EVENT_TOPICS.map((topic) =>
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

  return (
    <I18nextProvider i18n={i18n}>
      <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>
    </I18nextProvider>
  );
}
