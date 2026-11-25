import { QueryClient, QueryObserver } from '@tanstack/react-query';
import { describe, expect, it } from 'vitest';
import { invalidateEventProjectionQueries } from '@/features/cache-invalidation';
import { ANALYSIS_QUERY_OPTIONS } from '@/features/analysis/query-options';

describe('published analysis cache', () => {
  it.each(['ready', 'partial'] as const)('refreshes an inactive zero result after %s publication', async (state) => {
    const client = new QueryClient();
    const queryKey = ['analysis', 'browser-history', 'case-a', 'source-a', 'windows', 0, 200];
    const foreignKey = ['analysis', 'browser-history', 'case-a', 'source-b', 'windows', 0, 200];
    const options = {
      queryKey,
      ...ANALYSIS_QUERY_OPTIONS,
      queryFn: async () => client.getQueryData<number>(['published-count', 'source-a']) ?? 0,
    };
    client.setQueryData(foreignKey, 0);
    await client.fetchQuery(options);
    expect(client.getQueryData(queryKey)).toBe(0);

    client.setQueryData(['published-count', 'source-a'], 1695);
    invalidateEventProjectionQueries(client, 'capability-status-changed', {
      caseId: 'case-a',
      dataSourceId: 'source-a',
      capability: 'artifacts',
      state,
    });
    expect(client.getQueryData(queryKey)).toBe(0);
    const observer = new QueryObserver(client, options);
    const refreshed = new Promise<number>((resolve) => {
      const unsubscribe = observer.subscribe((result) => {
        if (!result.isFetching && result.data === 1695) {
          unsubscribe();
          resolve(result.data);
        }
      });
    });
    expect(await refreshed).toBe(1695);
    expect(client.getQueryState(foreignKey)?.isInvalidated).toBe(false);
    client.clear();
  });

  it('reuses a current result when the page mounts again', async () => {
    const client = new QueryClient();
    let reads = 0;
    const options = {
      queryKey: ['analysis', 'browser-history', 'case-a', 'source-a'],
      ...ANALYSIS_QUERY_OPTIONS,
      queryFn: async () => {
        reads += 1;
        return 1695;
      },
    };
    await client.fetchQuery(options);
    const observer = new QueryObserver(client, options);
    const unsubscribe = observer.subscribe(() => {});
    expect(observer.getCurrentResult().data).toBe(1695);
    expect(observer.getCurrentResult().isFetching).toBe(false);
    expect(reads).toBe(1);
    unsubscribe();
    client.clear();
  });

  it('replaces an initial query still reading the previous generation at publication', async () => {
    const client = new QueryClient();
    let finishPrevious: ((count: number) => void) | undefined;
    const previous = new Promise<number>((resolve) => {
      finishPrevious = resolve;
    });
    let reads = 0;
    const options = {
      queryKey: ['analysis', 'browser-history', 'case-a', 'source-a'],
      ...ANALYSIS_QUERY_OPTIONS,
      queryFn: () => {
        reads += 1;
        return reads === 1 ? previous : Promise.resolve(1695);
      },
    };
    const observer = new QueryObserver(client, options);
    const unsubscribe = observer.subscribe(() => {});
    expect(reads).toBe(1);
    invalidateEventProjectionQueries(client, 'capability-status-changed', {
      caseId: 'case-a',
      dataSourceId: 'source-a',
      capability: 'artifacts',
      state: 'partial',
    });
    finishPrevious?.(0);
    await previous;
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(observer.getCurrentResult().data).toBe(1695);
    expect(reads).toBe(2);
    unsubscribe();
    client.clear();
  });
});
