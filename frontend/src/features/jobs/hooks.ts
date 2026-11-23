import { useEffect } from 'react';
import { useQuery, useQueryClient, type QueryClient } from '@tanstack/react-query';
import { getJobsSnapshot, getTraceItems, getWarnings } from '@/lib/api/jobs';
import { invalidatePostJobProjectionQueries } from '@/features/cache-invalidation';
import type { JobSnapshot } from '@/types/models';
import { isActiveJob, jobsSnapshotPollInterval, mergeJobSnapshots } from './job-snapshot-events';

const JOB_HANDOFF_POLL_WINDOW_MS = 10000;
interface JobObservation {
  runningJobIds: Set<string>;
  refreshedSettledJobIds: Set<string>;
}
const observedCases = new WeakMap<QueryClient, Map<string, JobObservation>>();
let handoffBaselineJobIds = new Set<string>();
let pollJobsUntil = 0;

function hasRunningJobs(jobs?: JobSnapshot[]) {
  return jobs?.some(isActiveJob) ?? false;
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
  invalidatePostJobProjectionQueries(queryClient);
}

function refreshWhenObservedJobsSettle(queryClient: QueryClient, caseId?: string, jobs?: JobSnapshot[]) {
  if (!jobs || !caseId) {
    return;
  }
  let cases = observedCases.get(queryClient);
  if (!cases) {
    cases = new Map();
    observedCases.set(queryClient, cases);
  }
  let observation = cases.get(caseId);
  if (!observation) {
    observation = { runningJobIds: new Set(), refreshedSettledJobIds: new Set() };
    cases.set(caseId, observation);
  }
  const { runningJobIds, refreshedSettledJobIds } = observation;

  const nextRunningJobIds = new Set(jobs.filter(isActiveJob).map((job) => job.id));
  const settledJobIds = Array.from(runningJobIds).filter(
    (jobId) => !nextRunningJobIds.has(jobId) && !refreshedSettledJobIds.has(jobId),
  );
  const newlySettledHandoffJobIds =
    Date.now() < pollJobsUntil
      ? jobs
          .filter(
            (job) => !isActiveJob(job) && !handoffBaselineJobIds.has(job.id) && !refreshedSettledJobIds.has(job.id),
          )
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

  // A settled job that has left the snapshot can never be observed running again, so drop
  // it to keep this set bounded for the lifetime of the session.
  if (refreshedSettledJobIds.size > 0) {
    const visibleJobIds = new Set(jobs.map((job) => job.id));
    refreshedSettledJobIds.forEach((jobId) => {
      if (!visibleJobIds.has(jobId)) {
        refreshedSettledJobIds.delete(jobId);
      }
    });
  }
}

export function useJobsSnapshot(caseId?: string) {
  const queryClient = useQueryClient();
  const query = useQuery({
    queryKey: ['jobs', 'snapshot', caseId ?? null],
    queryFn: getJobsSnapshot,
    enabled: Boolean(caseId),
    structuralSharing: mergeJobSnapshots,
    refetchOnWindowFocus: true,
    refetchInterval: (activeQuery) =>
      shouldPollJobs(activeQuery.state.data as JobSnapshot[] | undefined)
        ? jobsSnapshotPollInterval(queryClient, caseId ?? '')
        : false,
    refetchIntervalInBackground: true,
  });

  useEffect(() => {
    refreshWhenObservedJobsSettle(queryClient, caseId, query.data);
  }, [queryClient, caseId, query.data]);

  return query;
}

export function useWarnings(caseId?: string) {
  return useQuery({ queryKey: ['jobs', 'warnings', caseId ?? null], queryFn: getWarnings });
}

export function useTraceItems(caseId?: string) {
  return useQuery({ queryKey: ['jobs', 'trace', caseId ?? null], queryFn: getTraceItems });
}
