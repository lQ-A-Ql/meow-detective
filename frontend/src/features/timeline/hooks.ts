import { useQuery } from '@tanstack/react-query';
import { getTimelineEvents, TimelineRequest } from '@/lib/api/timeline';
import { timelineQueryKeys } from '@/features/cache-invalidation';

export function useTimelineEvents(request?: TimelineRequest) {
  return useQuery({
    queryKey: timelineQueryKeys.events(request),
    queryFn: () => getTimelineEvents(request),
  });
}
