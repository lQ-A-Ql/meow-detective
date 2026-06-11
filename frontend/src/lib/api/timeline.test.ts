import { describe, it, expect } from 'vitest';
import { getTimelineEvents } from '@/lib/api/timeline';

describe('timeline API (mock mode)', () => {
  it('getTimelineEvents returns paged response', async () => {
    const result = await getTimelineEvents();
    expect(result).toBeDefined();
    expect(typeof result.total).toBe('number');
    expect(Array.isArray(result.items)).toBe(true);
  });

  it('getTimelineEvents returns events with required fields', async () => {
    const result = await getTimelineEvents();
    if (result.items.length > 0) {
      const event = result.items[0];
      expect(event.id).toBeDefined();
      expect(event.sourceObjectId).toBeDefined();
      expect(event.eventType).toBeDefined();
      expect(event.ts).toBeDefined();
      expect(event.title).toBeDefined();
      expect(event.description).toBeDefined();
    }
  });

  it('getTimelineEvents accepts request parameters', async () => {
    const result = await getTimelineEvents({
      offset: 0,
      limit: 10,
      timeStart: '2026-05-01T00:00:00Z',
      timeEnd: '2026-06-01T00:00:00Z',
      eventType: 'Logon 4624',
    });
    expect(result).toBeDefined();
    expect(Array.isArray(result.items)).toBe(true);
  });

  it('getTimelineEvents returns total matching items count', async () => {
    const result = await getTimelineEvents();
    expect(result.total).toBe(result.items.length);
  });

  it('getTimelineEvents events have parser metadata', async () => {
    const result = await getTimelineEvents();
    for (const event of result.items) {
      expect(typeof event.sourceObjectId).toBe('string');
      expect(typeof event.eventType).toBe('string');
    }
  });
});
