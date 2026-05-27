import { useEffect } from 'react';
import { useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import { getJobsSnapshot, getTraceItems, getWarnings } from '@/lib/api/jobs';
import type { JobSnapshot } from '@/types/models';

const JOB_POLL_INTERVAL_MS = 1500;
const JOB_HANDOFF_POLL_WINDOW_MS = 10000;
const runningJobIds = new Set<string>();
const refreshedSettledJobIds = new Set<string>();
let handoffBaselineJobIds = new Set<string>();
let pollJobsUntil = 0;

const postJobRefreshKeys = [
  ['case'],
  ['files'],
  ['timeline'],
  ['artifacts'],
  ['search'],
  ['jobs', 'warnings'],
  ['jobs', 'trace'],
] as const;

function hasRunningJobs(jobs?: JobSnapshot[]) {
  return jobs?.some((job) => job.status === 'running') ?? false;
}

function shouldPollJobs(jobs?: JobSnapshot[]) {
  return hasRunningJobs(jobs) || Date.now() < pollJobsUntil;
}

export function expectJobsSnapshotActivity(baselineJobs?: JobSnapshot[], windowMs = JOB_HANDOFF_POLL_WINDOW_MS) {
  pollJobsUntil = Math.max(pollJobsUntil, Date.now() + windowMs);

  if (baselineJobs) {
    handoffBaselineJobIds = new Set(baselineJobs.map((job) => job.id));
  }
}

function invalidatePostJobQueries(queryClient: QueryClient) {
  postJobRefreshKeys.forEach((queryKey) => {
    queryClient.invalidateQueries({ queryKey });
  });
}

function refreshWhenObservedJobsSettle(queryClient: QueryClient, jobs?: JobSnapshot[]) {
  if (!jobs) {
    return;
  }

  const nextRunningJobIds = new Set(jobs.filter((job) => job.status === 'running').map((job) => job.id));
  const settledJobIds = Array.from(runningJobIds).filter(
    (jobId) => !nextRunningJobIds.has(jobId) && !refreshedSettledJobIds.has(jobId),
  );
  const newlySettledHandoffJobIds =
    Date.now() < pollJobsUntil
      ? jobs
          .filter((job) => job.status !== 'running' && !handoffBaselineJobIds.has(job.id) && !refreshedSettledJobIds.has(job.id))
          .map((job) => job.id)
      : [];

  if (settledJobIds.length > 0 || newlySettledHandoffJobIds.length > 0) {
    [...settledJobIds, ...newlySettledHandoffJobIds].forEach((jobId) => refreshedSettledJobIds.add(jobId));
    invalidatePostJobQueries(queryClient);
  }

  runningJobIds.clear();
  nextRunningJobIds.forEach((jobId) => {
    refreshedSettledJobIds.delete(jobId);
    runningJobIds.add(jobId);
  });
}

export function useJobsSnapshot() {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: ['jobs', 'snapshot'],
    queryFn: getJobsSnapshot,
    refetchInterval: (activeQuery) => (shouldPollJobs(activeQuery.state.data as JobSnapshot[] | undefined) ? JOB_POLL_INTERVAL_MS : false),
    refetchIntervalInBackground: true,
  });

  useEffect(() => {
    refreshWhenObservedJobsSettle(queryClient, query.data);
  }, [queryClient, query.data]);

  return query;
}

export function useWarnings() {
  return useQuery({ queryKey: ['jobs', 'warnings'], queryFn: getWarnings });
}

export function useTraceItems() {
  return useQuery({ queryKey: ['jobs', 'trace'], queryFn: getTraceItems });
}
