import { useQuery, useQueryClient, useMutation } from '@tanstack/react-query';
import {
  createBatchPlan,
  startBatch,
  pauseBatch,
  resumeBatch,
  cancelBatch,
  getBatchJob,
  listBatchJobs,
} from '@/lib/api/batch';
import type { BatchJob, BatchPlan } from '@/types/models';

const BATCH_POLL_INTERVAL_MS = 1500;

const runningBatchIds = new Set<string>();

function hasRunningBatch(jobs?: BatchJob[]) {
  return jobs?.some((job) => job.status === 'running') ?? false;
}

export function useListBatchJobs() {
  const query = useQuery({
    queryKey: ['batch', 'list'],
    queryFn: listBatchJobs,
    refetchInterval: (activeQuery) => {
      const data = activeQuery.state.data as BatchJob[] | undefined;
      return hasRunningBatch(data) ? BATCH_POLL_INTERVAL_MS : false;
    },
    refetchIntervalInBackground: true,
  });

  return query;
}

export function useBatchJob(jobId: string | null) {
  return useQuery({
    queryKey: ['batch', 'job', jobId],
    queryFn: () => getBatchJob(jobId!),
    enabled: jobId !== null,
    refetchInterval: (activeQuery) => {
      const data = activeQuery.state.data as BatchJob | null | undefined;
      return data?.status === 'running' ? BATCH_POLL_INTERVAL_MS : false;
    },
    refetchIntervalInBackground: true,
  });
}

export function useCreateBatchPlan() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (plan: BatchPlan) => createBatchPlan(plan),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['batch', 'list'] });
    },
  });
}

export function useStartBatch() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (jobId: string) => startBatch(jobId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['batch'] });
    },
  });
}

export function usePauseBatch() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (jobId: string) => pauseBatch(jobId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['batch'] });
    },
  });
}

export function useResumeBatch() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (jobId: string) => resumeBatch(jobId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['batch'] });
    },
  });
}

export function useCancelBatch() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: (jobId: string) => cancelBatch(jobId),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['batch'] });
    },
  });
}
