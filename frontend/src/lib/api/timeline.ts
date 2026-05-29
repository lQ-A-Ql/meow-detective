import { apiClient } from './client';
import { TimelineEventDto } from '@/types/models';

export interface PagedResponse<T> {
  total: number;
  items: T[];
}

export interface TimelineRequest {
  offset?: number;
  limit?: number;
  timeStart?: string;
  timeEnd?: string;
  eventType?: string;
}

export async function getTimelineEvents(
  request?: TimelineRequest,
): Promise<PagedResponse<TimelineEventDto>> {
  return apiClient.request('get_timeline_events', () =>
    apiClient.getMockProvider().getTimelineEvents().then((items) => ({
      total: items.length,
      items,
    })),
    request ? { request: {
      offset: request.offset ?? 0,
      limit: request.limit ?? 100,
      timeStart: request.timeStart,
      timeEnd: request.timeEnd,
      eventType: request.eventType,
    } } : undefined,
  );
}
