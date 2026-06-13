import { useQuery } from '@tanstack/react-query';
import { getTimelineEventById, getTimelineEvents, TimelineRequest } from '@/lib/api/timeline';
import { timelineQueryKeys } from '@/features/cache-invalidation';

export function useTimelineEvents(request?: TimelineRequest) {
  return useQuery({
    queryKey: timelineQueryKeys.events(request),
    queryFn: () => getTimelineEvents(request),
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
