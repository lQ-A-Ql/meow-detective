import { COMMANDS } from './commands';
import { apiClient } from './client';
import { TimelineEvent } from '@/types/models';

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
): Promise<PagedResponse<TimelineEvent>> {
  return apiClient.request(COMMANDS.timeline.GET_TIMELINE_EVENTS, request ? { request: {
      offset: request.offset ?? 0,
      limit: request.limit ?? 100,
      timeStart: request.timeStart,
      timeEnd: request.timeEnd,
      eventType: request.eventType,
    } } : undefined);
}

export async function getTimelineEventById(eventId: string): Promise<TimelineEvent | null> {
  return apiClient.request(COMMANDS.timeline.GET_TIMELINE_EVENT_BY_ID, { request: { eventId } });
}
