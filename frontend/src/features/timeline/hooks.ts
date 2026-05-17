import { useQuery } from '@tanstack/react-query';
import { getTimelineEvents } from '@/lib/api/timeline';

export function useTimelineEvents() {
  return useQuery({ queryKey: ['timeline', 'events'], queryFn: getTimelineEvents });
}
