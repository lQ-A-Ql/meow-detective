import { QueryClient } from '@tanstack/react-query';
import { describe, expect, it, vi } from 'vitest';
import {
  invalidateCacheStatusQueries,
  invalidateEventProjectionQueries,
  invalidateImportProjectionQueries,
  invalidatePartialResultQueries,
  invalidatePostJobProjectionQueries,
  timelineQueryKeys,
} from './cache-invalidation';
import type { PartialResult } from '@/types/models';

function createClient() {
  const queryClient = new QueryClient();
  const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
  return { queryClient, invalidateSpy };
}

function invalidatedKeys(invalidateSpy: ReturnType<typeof vi.spyOn>) {
  return invalidateSpy.mock.calls.map(([filters]: [{ queryKey?: unknown }?]) => filters?.queryKey);
}

describe('projection cache invalidation', () => {
  it('uses one canonical timeline events query key builder', () => {
    expect(timelineQueryKeys.events()).toEqual(['timeline', 'events', 0, 100, null, null, null]);
    expect(timelineQueryKeys.events({ offset: 10, limit: 25, timeStart: 'a', timeEnd: 'b', eventType: 'FileAccess' })).toEqual([
      'timeline',
      'events',
      10,
      25,
      'a',
      'b',
      'FileAccess',
    ]);
  });

  it('invalidates the import projections used by files, timeline, artifacts, and search', () => {
    const { queryClient, invalidateSpy } = createClient();

    invalidateImportProjectionQueries(queryClient);

    expect(invalidatedKeys(invalidateSpy)).toEqual([
      ['case', 'data-sources'],
      ['case', 'metrics'],
      ['case', 'recent-objects'],
      ['files'],
      ['timeline'],
      ['artifacts'],
      ['search'],
    ]);
  });

  it('invalidates post-job projections and job diagnostic queries', () => {
    const { queryClient, invalidateSpy } = createClient();

    invalidatePostJobProjectionQueries(queryClient);

    expect(invalidatedKeys(invalidateSpy)).toEqual([
      ['case', 'data-sources'],
      ['case', 'metrics'],
      ['case', 'recent-objects'],
      ['files'],
      ['timeline'],
      ['artifacts'],
      ['search'],
      ['jobs', 'warnings'],
      ['jobs', 'trace'],
    ]);
  });

  it('maps ready import partial results to key-aware projection invalidations', () => {
    const { queryClient, invalidateSpy } = createClient();
    const result: Pick<PartialResult, 'kind' | 'freshness'> = { kind: 'timelineEvents', freshness: 'ready' };

    invalidatePartialResultQueries(queryClient, result);

    expect(invalidatedKeys(invalidateSpy)).toEqual([['timeline']]);
  });

  it('does not invalidate deferred partial results', () => {
    const { queryClient, invalidateSpy } = createClient();
    const result: Pick<PartialResult, 'kind' | 'freshness'> = { kind: 'timelineBuckets', freshness: 'deferred' };

    invalidatePartialResultQueries(queryClient, result);

    expect(invalidateSpy).not.toHaveBeenCalled();
  });

  it('routes backend projection events to affected query families', () => {
    const { queryClient, invalidateSpy } = createClient();

    invalidateEventProjectionQueries(queryClient, 'artifact-added');
    invalidateEventProjectionQueries(queryClient, 'timeline-updated');
    invalidateEventProjectionQueries(queryClient, 'search-index-progress');

    expect(invalidatedKeys(invalidateSpy)).toEqual([['artifacts'], ['timeline'], ['timeline'], ['search']]);
  });

  it('routes cache status updates by cache key when materialized', () => {
    const { queryClient, invalidateSpy } = createClient();

    invalidateCacheStatusQueries(queryClient, { cacheKey: 'timeline:buckets:case-1', state: 'ready' });
    invalidateCacheStatusQueries(queryClient, { cacheKey: 'search:index:case-1', state: 'pending' });

    expect(invalidatedKeys(invalidateSpy)).toEqual([['timeline']]);
  });
});
