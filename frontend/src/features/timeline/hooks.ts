import { useInfiniteQuery, useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import {
  getTimelineEventById,
  getTimelineEvents,
  getTimelineFacets,
  getTimelineProjectionStatus,
  rebuildTimelineProjection,
  TimelineRequest,
} from '@/lib/api/timeline';
import type { TimelineFacetsRequest } from '@/types/timeline';
import { timelineQueryKeys } from '@/features/cache-invalidation';
import { useCurrentCase } from '@/features/case/hooks';

function scopeKey(key: readonly unknown[], caseId: string | undefined) {
  return [key[0], caseId ?? null, ...key.slice(1)];
}

export function useTimelineEvents(request?: TimelineRequest) {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: scopeKey(timelineQueryKeys.events(request), currentCase.data?.id),
    queryFn: () => getTimelineEvents(request),
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
  });
}

export function useInfiniteTimelineEvents(
  request?: Omit<TimelineRequest, 'offset' | 'limit' | 'cursor'>,
  pageSize = 200,
) {
  const currentCase = useCurrentCase();
  return useInfiniteQuery({
    queryKey: [
      ...scopeKey(timelineQueryKeys.events({ ...request, limit: pageSize }), currentCase.data?.id),
      'infinite',
    ],
    initialPageParam: undefined as string | undefined,
    queryFn: ({ pageParam }) =>
      getTimelineEvents({
        ...request,
        limit: pageSize,
        cursor: pageParam,
      }),
    getNextPageParam: (lastPage) => lastPage.nextCursor,
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
  });
}

export function useTimelineEventById(eventId?: string) {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: ['timeline', currentCase.data?.id ?? null, 'event-by-id', eventId ?? null],
    queryFn: () => getTimelineEventById(eventId!),
    enabled: currentCase.isSuccess && Boolean(currentCase.data) && Boolean(eventId),
    retry: false,
  });
}

export function useTimelineFacets(request?: TimelineFacetsRequest) {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: scopeKey(timelineQueryKeys.facets(request), currentCase.data?.id),
    queryFn: () => getTimelineFacets(request),
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
  });
}

const PROJECTION_STATUS_KEY = ['timeline', 'projection-status'] as const;

export function useTimelineProjectionStatus() {
  const currentCase = useCurrentCase();
  return useQuery({
    queryKey: scopeKey(PROJECTION_STATUS_KEY, currentCase.data?.id),
    queryFn: () => getTimelineProjectionStatus(),
    enabled: currentCase.isSuccess && Boolean(currentCase.data),
    refetchInterval: (query) => (query.state.data?.state === 'building' ? 3000 : false),
  });
}

export function useRebuildTimelineProjection() {
  const currentCase = useCurrentCase();
  const queryClient = useQueryClient();
  return useMutation({
    mutationFn: (dataSourceId?: string) => rebuildTimelineProjection(dataSourceId),
    onSuccess: async (status) => {
      queryClient.setQueryData(scopeKey(PROJECTION_STATUS_KEY, currentCase.data?.id), status);
      await queryClient.invalidateQueries({ queryKey: timelineQueryKeys.root });
    },
  });
}
