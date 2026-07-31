import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getTimelineEvents: vi.fn(),
  getTimelineEventById: vi.fn(),
  getTimelineFacets: vi.fn(),
}));

vi.mock('@/lib/api/timeline', () => ({
  getTimelineEvents: mocks.getTimelineEvents,
  getTimelineEventById: mocks.getTimelineEventById,
  getTimelineFacets: mocks.getTimelineFacets,
}));

vi.mock('@/features/cache-invalidation', async (importOriginal) => {
  const orig = await importOriginal<typeof import('@/features/cache-invalidation')>();
  return {
    ...orig,
    timelineQueryKeys: {
      root: ['timeline'],
      eventsRoot: ['timeline', 'events'],
      events: (request?: {
        offset?: number;
        limit?: number;
        timeStart?: string;
        timeEnd?: string;
        eventType?: string;
      }) => [
        'timeline',
        'events',
        request?.offset ?? 0,
        request?.limit ?? 100,
        request?.timeStart ?? null,
        request?.timeEnd ?? null,
        request?.eventType ?? null,
      ],
      facets: (request?: {
        timeStart?: string;
        timeEnd?: string;
        eventType?: string;
        bucketCount?: number;
      }) => [
        'timeline',
        'facets',
        request?.timeStart ?? null,
        request?.timeEnd ?? null,
        request?.eventType ?? null,
        request?.bucketCount ?? 60,
      ],
    },
  };
});

import {
  useInfiniteTimelineEvents,
  useTimelineEvents,
  useTimelineEventById,
  useTimelineFacets,
} from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('timeline hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useTimelineEvents', () => {
    it('fetches timeline events with default paging', async () => {
      const paged = { total: 1, items: [{ id: 'evt-1', eventType: 'FileCreate' }] };
      mocks.getTimelineEvents.mockResolvedValue(paged);

      const { result } = renderHook(() => useTimelineEvents(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data).toEqual(paged);
      expect(mocks.getTimelineEvents).toHaveBeenCalledWith(undefined);
    });

    it('passes filter params to the API', async () => {
      const paged = { total: 0, items: [] };
      mocks.getTimelineEvents.mockResolvedValue(paged);

      const request = {
        offset: 0,
        limit: 50,
        timeStart: '2026-01-01T00:00:00Z',
        timeEnd: '2026-06-01T00:00:00Z',
        eventType: 'FileCreate',
      };

      const { result } = renderHook(() => useTimelineEvents(request), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(mocks.getTimelineEvents).toHaveBeenCalledWith(request);
    });

    it('handles API errors', async () => {
      mocks.getTimelineEvents.mockRejectedValue(new Error('timeout'));

      const { result } = renderHook(() => useTimelineEvents({ limit: 100 }), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isError).toBe(true));
      expect((result.current.error as Error).message).toBe('timeout');
    });
  });

  describe('useInfiniteTimelineEvents', () => {
    it('uses the opaque cursor as the next page parameter', async () => {
      mocks.getTimelineEvents
        .mockResolvedValueOnce({
          total: 3,
          items: [{ id: 'evt-1' }, { id: 'evt-2' }],
          nextCursor: 'timeline-cursor-1',
        })
        .mockResolvedValueOnce({
          total: 3,
          items: [{ id: 'evt-3' }],
        });

      const request = { eventType: 'FileCreate' };
      const { result } = renderHook(
        () => useInfiniteTimelineEvents(request, 2),
        { wrapper: createWrapper() },
      );

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(mocks.getTimelineEvents).toHaveBeenNthCalledWith(1, {
        eventType: 'FileCreate',
        limit: 2,
        cursor: undefined,
      });

      let nextResult: Awaited<ReturnType<typeof result.current.fetchNextPage>> | undefined;
      await act(async () => {
        nextResult = await result.current.fetchNextPage();
      });

      expect(mocks.getTimelineEvents).toHaveBeenNthCalledWith(2, {
        eventType: 'FileCreate',
        limit: 2,
        cursor: 'timeline-cursor-1',
      });
      expect(nextResult?.data?.pages.flatMap((page) => page.items)).toHaveLength(3);
      expect(nextResult?.hasNextPage).toBe(false);
    });

    it('stops timeline pagination when the backend omits nextCursor', async () => {
      mocks.getTimelineEvents.mockResolvedValueOnce({
        total: 3,
        items: [{ id: 'evt-1' }, { id: 'evt-2' }],
      });
      const { result } = renderHook(() => useInfiniteTimelineEvents(undefined, 2), {
        wrapper: createWrapper(),
      });
      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.hasNextPage).toBe(false);
      expect(mocks.getTimelineEvents).toHaveBeenCalledTimes(1);
    });
  });

  describe('useTimelineEventById', () => {
    it('is disabled when no eventId is provided', () => {
      const { result } = renderHook(() => useTimelineEventById(), {
        wrapper: createWrapper(),
      });

      expect(result.current.fetchStatus).toBe('idle');
      expect(mocks.getTimelineEventById).not.toHaveBeenCalled();
    });

    it('fetches a single event by id', async () => {
      const event = { id: 'evt-42', eventType: 'FileDelete' };
      mocks.getTimelineEventById.mockResolvedValue(event);

      const { result } = renderHook(() => useTimelineEventById('evt-42'), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data).toEqual(event);
      expect(mocks.getTimelineEventById).toHaveBeenCalledWith('evt-42');
    });
  });

  describe('useTimelineFacets', () => {
    it('fetches server-side timeline facets', async () => {
      const facets = {
        totalEvents: 3,
        eventTypes: [{ value: 'FILE_MODIFIED', count: 3 }],
        dataSources: [{ value: 'source-1', count: 3 }],
        histogram: [],
      };
      mocks.getTimelineFacets.mockResolvedValue(facets);

      const { result } = renderHook(
        () => useTimelineFacets({ bucketCount: 60 }),
        { wrapper: createWrapper() },
      );

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data).toEqual(facets);
      expect(mocks.getTimelineFacets).toHaveBeenCalledWith({ bucketCount: 60 });
    });
  });
});
