import { useQuery } from '@tanstack/react-query';
import { getTimelineEvents, TimelineRequest } from '@/lib/api/timeline';

export function useTimelineEvents(request?: TimelineRequest) {
  return useQuery({
    queryKey: ['timeline', 'events', request?.offset ?? 0, request?.limit ?? 100],
    queryFn: () => getTimelineEvents(request),
  });
}
