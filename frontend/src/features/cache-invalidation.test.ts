import { QueryClient } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import {
  PROJECTION_EVENT_TOPICS,
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

function invalidatedKeys(invalidateSpy: ReturnType<typeof vi.spyOn>): unknown[][] {
  return invalidateSpy.mock.calls.map(([filters]: [{ queryKey?: unknown }?]) => {
    const key = filters?.queryKey;
    return Array.isArray(key) ? key : [];
  });
}

describe('projection cache invalidation', () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.clearAllTimers();
    vi.useRealTimers();
  });

  it('subscribes every terminal job event to projection invalidation', () => {
    expect(PROJECTION_EVENT_TOPICS).toEqual(expect.arrayContaining([
      'job-completed',
      'job-failed',
      'job-cancelled',
    ]));
  });

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

  it('invalidates the import projections used by files, timeline, artifacts, search, and analysis', () => {
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
      ['analysis'],
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
      ['analysis'],
      ['jobs', 'warnings'],
      ['jobs', 'trace'],
    ]);
  });

  it('maps ready import partial results to key-aware projection invalidations', () => {
    const { queryClient, invalidateSpy } = createClient();
    const result: Pick<PartialResult, 'kind' | 'freshness'> = { kind: 'timelineEvents', freshness: 'ready' };

    invalidatePartialResultQueries(queryClient, result);
    expect(invalidateSpy).not.toHaveBeenCalled();

    vi.advanceTimersByTime(300);

    expect(invalidatedKeys(invalidateSpy)).toEqual([['timeline']]);
  });

  it('refreshes case data sources when evidence hashing completes', () => {
    const { queryClient, invalidateSpy } = createClient();
    const result: Pick<PartialResult, 'kind' | 'freshness'> = { kind: 'evidenceHash', freshness: 'ready' };

    invalidatePartialResultQueries(queryClient, result);
    vi.advanceTimersByTime(300);

    expect(invalidatedKeys(invalidateSpy)).toEqual([
      ['case', 'metrics'],
      ['case', 'data-sources'],
      ['reports'],
    ]);
  });

  it('does not invalidate deferred partial results', () => {
    const { queryClient, invalidateSpy } = createClient();
    const result: Pick<PartialResult, 'kind' | 'freshness'> = { kind: 'timelineBuckets', freshness: 'deferred' };

    invalidatePartialResultQueries(queryClient, result);
    vi.advanceTimersByTime(600);

    expect(invalidateSpy).not.toHaveBeenCalled();
  });

  it('coalesces high-frequency backend projection events into one invalidation per family', () => {
    const { queryClient, invalidateSpy } = createClient();

    invalidateEventProjectionQueries(queryClient, 'artifact-added');
    invalidateEventProjectionQueries(queryClient, 'timeline-updated');
    invalidateEventProjectionQueries(queryClient, 'search-index-progress');
    expect(invalidateSpy).not.toHaveBeenCalled();

    vi.advanceTimersByTime(300);

    expect(invalidatedKeys(invalidateSpy)).toEqual([['artifacts'], ['timeline'], ['search']]);
  });

  it('refreshes projections when a background job reaches a terminal state', () => {
    const { queryClient, invalidateSpy } = createClient();

    invalidateEventProjectionQueries(queryClient, 'job-completed');
    invalidateEventProjectionQueries(queryClient, 'job-failed');
    invalidateEventProjectionQueries(queryClient, 'job-cancelled');

    const keys = invalidatedKeys(invalidateSpy);
    expect(keys).toHaveLength(30);
    expect(keys.filter((key) => Array.isArray(key) && key[0] === 'analysis')).toHaveLength(3);
    expect(keys.filter((key) => Array.isArray(key) && key[0] === 'jobs')).toHaveLength(6);
  });

  it('routes cache status updates by cache key when materialized', () => {
    const { queryClient, invalidateSpy } = createClient();

    invalidateCacheStatusQueries(queryClient, { cacheKey: 'timeline:buckets:case-1', state: 'ready' });
    invalidateCacheStatusQueries(queryClient, { cacheKey: 'search:index:case-1', state: 'pending' });
    vi.advanceTimersByTime(300);

    expect(invalidatedKeys(invalidateSpy)).toEqual([['timeline']]);
  });
});
