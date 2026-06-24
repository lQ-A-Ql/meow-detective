import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getTimelineEvents: vi.fn(),
  getTimelineEventById: vi.fn(),
}));

vi.mock('@/lib/api/timeline', () => ({
  getTimelineEvents: mocks.getTimelineEvents,
  getTimelineEventById: mocks.getTimelineEventById,
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
    },
  };
});

import { useTimelineEvents, useTimelineEventById } from './hooks';

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
});
