import type { QueryClient } from '@tanstack/react-query';
import { subscribeToEvent } from '@/lib/events/subscribers';
import type { ImportPhaseProgress, JobSnapshot, TaskProgress } from '@/types/models';

interface JobProgressPayload {
  jobId: string;
  progress: number;
  detail?: string;
}

interface PartitionProgressPayload {
  jobId: string;
  currentPartition: string;
  completedPartitions: number;
  totalPartitions: number;
  partitionProgress: number;
}

const lastEvents = new WeakMap<QueryClient, Map<string, number>>();
export const isActiveJob = (job: JobSnapshot) =>
  job.status === 'pending' || job.status === 'running' || job.status === 'cancelling';

export function jobsSnapshotPollInterval(client: QueryClient, caseId: string) {
  const timestamp = lastEvents.get(client)?.get(caseId);
  return timestamp !== undefined && Date.now() - timestamp < 15_000 ? 10_000 : 1500;
}

export function mergeJobSnapshots(previous: unknown, next: unknown): JobSnapshot[] {
  const jobs = new Map((previous as JobSnapshot[] | undefined)?.map((job) => [job.id, job]));
  return (next as JobSnapshot[]).map((job) => {
    const before = jobs.get(job.id);
    if (!before || !isActiveJob(before) || !isActiveJob(job)) return job;
    return { ...job, progress: Math.max(before.progress, job.progress) };
  });
}

export function subscribeToJobSnapshotEvents(client: QueryClient) {
  const seen = new Map<string, number>();
  lastEvents.set(client, seen);
  const timestamps = new Map<string, number>();

  function updateJob(jobId: string, ts: string, update: (job: JobSnapshot) => JobSnapshot, caseId?: string) {
    const timestamp = Date.parse(ts);
    if (!Number.isFinite(timestamp) || timestamp < (timestamps.get(jobId) ?? -Infinity)) return;
    timestamps.set(jobId, timestamp);
    for (const [key, jobs] of client.getQueriesData<JobSnapshot[]>({ queryKey: ['jobs', 'snapshot'] })) {
      if (caseId && key[2] !== caseId) continue;
      if (!jobs?.some((job) => job.id === jobId && isActiveJob(job))) continue;
      seen.set(String(key[2]), Date.now());
      client.setQueryData(
        key,
        jobs.map((job) => (job.id === jobId && isActiveJob(job) ? update(job) : job)),
      );
    }
  }

  const unsubs = [
    subscribeToEvent<JobProgressPayload | TaskProgress>('job-progress', ({ ts, payload }) => {
      if ('seq' in payload) return;
      updateJob(payload.jobId, ts, (job) => ({
        ...job,
        progress: Math.max(job.progress, payload.progress),
        detail: payload.detail ?? job.detail,
      }));
    }),
    subscribeToEvent<ImportPhaseProgress>('import-phase-progress', ({ ts, payload }) => {
      updateJob(
        payload.jobId,
        ts,
        (job) => ({ ...job, progress: Math.max(job.progress, payload.percent) }),
        payload.caseId,
      );
    }),
    subscribeToEvent<PartitionProgressPayload>('partition-progress', ({ ts, payload }) => {
      updateJob(payload.jobId, ts, (job) => ({ ...job, ...payload }));
    }),
    ...(['job-created', 'job-started', 'job-completed', 'job-failed', 'job-cancelled'] as const).map((topic) =>
      subscribeToEvent(topic, () => {
        void client.invalidateQueries({ queryKey: ['jobs', 'snapshot'] });
      }),
    ),
    subscribeToEvent('case-closed', () => {
      timestamps.clear();
      seen.clear();
    }),
  ];
  return () => {
    unsubs.forEach((unsubscribe) => unsubscribe());
    lastEvents.delete(client);
  };
}
