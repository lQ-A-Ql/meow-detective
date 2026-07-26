import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import { getTimelineEventById, getTimelineEvents } from './timeline';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('timeline API', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('getTimelineEvents sends request with paging defaults', async () => {
    requestMock.mockResolvedValueOnce({ total: 0, items: [] } as never);
    const result = await getTimelineEvents({});
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.timeline.GET_TIMELINE_EVENTS, {
      request: {
        offset: 0,
        limit: 100,
        timeStart: undefined,
        timeEnd: undefined,
        eventType: undefined,
        cursor: undefined,
      },
    });
    expect(result).toEqual({ total: 0, items: [] });
  });

  it('getTimelineEvents sends custom paging and filters', async () => {
    requestMock.mockResolvedValueOnce({ total: 5, items: [] } as never);
    await getTimelineEvents({
      offset: 10,
      limit: 25,
      timeStart: '2026-01-01T00:00:00Z',
      timeEnd: '2026-06-01T00:00:00Z',
      eventType: 'file_access',
    });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.timeline.GET_TIMELINE_EVENTS, {
      request: {
        offset: 10,
        limit: 25,
        timeStart: '2026-01-01T00:00:00Z',
        timeEnd: '2026-06-01T00:00:00Z',
        eventType: 'file_access',
        cursor: undefined,
      },
    });
  });

  it('getTimelineEvents forwards an opaque cursor', async () => {
    requestMock.mockResolvedValueOnce({ total: 5, items: [] } as never);
    await getTimelineEvents({ limit: 25, cursor: 'timeline-cursor-1' });
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.timeline.GET_TIMELINE_EVENTS, {
      request: {
        offset: 0,
        limit: 25,
        timeStart: undefined,
        timeEnd: undefined,
        eventType: undefined,
        cursor: 'timeline-cursor-1',
      },
    });
  });

  it('getTimelineEvents sends undefined payload when request is omitted', async () => {
    requestMock.mockResolvedValueOnce({ total: 0, items: [] } as never);
    await getTimelineEvents();
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.timeline.GET_TIMELINE_EVENTS, undefined);
  });

  it('getTimelineEventById sends eventId in request', async () => {
    requestMock.mockResolvedValueOnce({ id: 'evt-1' } as never);
    const result = await getTimelineEventById('evt-1');
    expect(requestMock).toHaveBeenCalledWith(COMMANDS.timeline.GET_TIMELINE_EVENT_BY_ID, {
      request: { eventId: 'evt-1' },
    });
    expect(result).toEqual({ id: 'evt-1' });
  });
});
