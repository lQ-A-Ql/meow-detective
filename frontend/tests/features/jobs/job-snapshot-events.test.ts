import { QueryClient } from '@tanstack/react-query';
import { describe, expect, it } from 'vitest';
import { eventBus } from '@/lib/events/bus';
import {
  jobsSnapshotPollInterval,
  mergeJobSnapshots,
  subscribeToJobSnapshotEvents,
} from '@/features/jobs/job-snapshot-events';
import type { JobSnapshot, TaskProgress } from '@/types/models';

const job: JobSnapshot = {
  id: 'import-1',
  name: 'Import',
  scope: '',
  status: 'running',
  progress: 30,
  detail: '',
  warningCount: 0,
  skippedCount: 0,
  failedCount: 0,
  partial: false,
};

describe('event-driven task snapshots', () => {
  it('keeps structured task heartbeats separate from partition snapshot ordering', () => {
    const client = new QueryClient();
    const key = ['jobs', 'snapshot', 'case-1'];
    client.setQueryData(key, [job]);
    const disconnect = subscribeToJobSnapshotEvents(client);
    const task: TaskProgress = {
      jobId: job.id,
      caseId: 'case-1',
      kind: 'import',
      seq: 3,
      state: 'running',
      progress: 40,
      mode: 'indeterminate',
      stage: 'index',
      stages: [{ key: 'index', weight: 100, state: 'running' }],
      completedUnits: 0,
      unit: 'items',
      elapsedMs: 10000,
      stageElapsedMs: 1000,
      heartbeatAt: '2026-11-24T04:00:02Z',
      heartbeat: true,
      cancellable: true,
    };
    try {
      eventBus.publishEvent({ eventId: 'heartbeat', topic: 'job-progress', ts: task.heartbeatAt, payload: task });
      expect(jobsSnapshotPollInterval(client, 'case-1')).toBe(1500);
      expect(client.getQueryData<JobSnapshot[]>(key)?.[0].progress).toBe(30);
      eventBus.publishEvent({
        eventId: 'partition',
        topic: 'partition-progress',
        ts: '2026-11-24T04:00:01Z',
        payload: {
          jobId: job.id,
          currentPartition: '2',
          completedPartitions: 1,
          totalPartitions: 2,
          partitionProgress: 50,
        },
      });
      expect(client.getQueryData<JobSnapshot[]>(key)?.[0].currentPartition).toBe('2');
    } finally {
      disconnect();
      client.clear();
    }
  });

  it('updates known jobs immediately and reduces polling while events arrive', () => {
    const client = new QueryClient();
    const key = ['jobs', 'snapshot', 'case-1'];
    client.setQueryData(key, [job]);
    client.setQueryData(['jobs', 'snapshot', 'case-2'], [{ ...job, id: 'other' }]);
    const disconnect = subscribeToJobSnapshotEvents(client);
    try {
      eventBus.publishEvent({
        eventId: 'progress',
        topic: 'job-progress',
        ts: '2026-11-23T10:00:02Z',
        payload: { jobId: job.id, progress: 50, detail: 'Enumerating' },
      });
      expect(client.getQueryData<JobSnapshot[]>(key)?.[0].progress).toBe(50);
      expect(jobsSnapshotPollInterval(client, 'case-1')).toBe(10_000);
      expect(jobsSnapshotPollInterval(client, 'case-2')).toBe(1500);
      eventBus.publishEvent({
        eventId: 'old',
        topic: 'job-progress',
        ts: '2026-11-23T10:00:01Z',
        payload: { jobId: job.id, progress: 80, detail: 'Older event' },
      });
      expect(client.getQueryData<JobSnapshot[]>(key)?.[0].detail).toBe('Enumerating');
      eventBus.publishEvent({
        eventId: 'complete',
        topic: 'job-completed',
        ts: '2026-11-23T10:00:03Z',
        payload: { jobId: job.id, message: '' },
      });
      expect(client.getQueryState(key)?.isInvalidated).toBe(true);
    } finally {
      disconnect();
      client.clear();
    }
  });

  it('preserves measured progress across snapshot races while accepting terminal snapshots', () => {
    expect(mergeJobSnapshots([{ ...job, progress: 70 }], [job])[0].progress).toBe(70);
    expect(
      mergeJobSnapshots([{ ...job, progress: 70 }], [{ ...job, status: 'completed', progress: 100 }])[0].status,
    ).toBe('completed');
  });

  it('keeps terminal jobs terminal when delayed progress arrives', () => {
    const client = new QueryClient();
    const key = ['jobs', 'snapshot', 'case-1'];
    client.setQueryData(key, [{ ...job, status: 'cancelled', progress: 60 }]);
    const disconnect = subscribeToJobSnapshotEvents(client);
    try {
      eventBus.publishEvent({
        eventId: 'late',
        topic: 'job-progress',
        ts: '2026-11-23T10:00:00Z',
        payload: { jobId: job.id, progress: 99, detail: '' },
      });
      expect(client.getQueryData<JobSnapshot[]>(key)?.[0].progress).toBe(60);
      expect(client.getQueryData<JobSnapshot[]>(key)?.[0].status).toBe('cancelled');
    } finally {
      disconnect();
      client.clear();
    }
  });
});
