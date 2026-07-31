import { useInfiniteQuery, useQuery } from '@tanstack/react-query';
import {
  getTimelineEventById,
  getTimelineEvents,
  getTimelineFacets,
  TimelineRequest,
} from '@/lib/api/timeline';
import type { TimelineFacetsRequest } from '@/types/timeline';
import { timelineQueryKeys } from '@/features/cache-invalidation';

export function useTimelineEvents(request?: TimelineRequest) {
  return useQuery({
    queryKey: timelineQueryKeys.events(request),
    queryFn: () => getTimelineEvents(request),
  });
}

export function useInfiniteTimelineEvents(
  request?: Omit<TimelineRequest, 'offset' | 'limit' | 'cursor'>,
  pageSize = 200,
) {
  return useInfiniteQuery({
    queryKey: [...timelineQueryKeys.events({ ...request, limit: pageSize }), 'infinite'],
    initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam }) => getTimelineEvents({
      ...request,
      limit: pageSize,
      cursor: pageParam,
    }),
    getNextPageParam: (lastPage) => lastPage.nextCursor,
  });
}

export function useTimelineEventById(eventId?: string) {
  return useQuery({
    queryKey: ['timeline', 'event-by-id', eventId ?? null],
    queryFn: () => getTimelineEventById(eventId!),
    enabled: Boolean(eventId),
    retry: false,
  });
}

export function useTimelineFacets(request?: TimelineFacetsRequest) {
  return useQuery({
    queryKey: timelineQueryKeys.facets(request),
    queryFn: () => getTimelineFacets(request),
  });
}
