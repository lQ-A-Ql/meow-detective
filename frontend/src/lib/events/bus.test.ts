import { describe, expect, it, vi } from 'vitest';
import { EventEnvelope, EventTopic } from '@/types/models';
import { eventBus } from './bus';

function event<T>(topic: EventTopic, payload: T): EventEnvelope<T> {
  return {
    eventId: `event-${topic}`,
    topic,
    ts: '2026-06-02T00:00:00Z',
    payload,
  };
}

describe('EventBus typed topics', () => {
  it('publishes data source imported and job cancelled events', () => {
    const imported = vi.fn();
    const cancelled = vi.fn();
    const unsubscribeImported = eventBus.subscribeEvent('data-source-imported', imported);
    const unsubscribeCancelled = eventBus.subscribeEvent('job-cancelled', cancelled);

    eventBus.publishEvent(event('data-source-imported', {
      dataSourceId: 'ds-1',
      name: 'Evidence',
      kind: 'logical_directory',
      jobId: 'job-1',
    }));
    eventBus.publishEvent(event('job-cancelled', {
      jobId: 'job-1',
      reason: 'Cancel requested by user',
    }));

    expect(imported).toHaveBeenCalledWith(expect.objectContaining({
      topic: 'data-source-imported',
      payload: expect.objectContaining({ dataSourceId: 'ds-1' }),
    }));
    expect(cancelled).toHaveBeenCalledWith(expect.objectContaining({
      topic: 'job-cancelled',
      payload: expect.objectContaining({ jobId: 'job-1' }),
    }));

    unsubscribeImported();
    unsubscribeCancelled();
  });
});
