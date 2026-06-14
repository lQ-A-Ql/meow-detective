import type { BatchJob, BatchPlan } from '@/types/models';
import { apiClient } from './client';

export async function createBatchPlan(plan: BatchPlan) {
  return apiClient.request('create_batch_plan', () =>
    apiClient.getMockProvider().createBatchPlan(plan),
    plan as unknown as Record<string, unknown>,
  );
}

export async function startBatch(jobId: string) {
  return apiClient.request('start_batch', () =>
    apiClient.getMockProvider().startBatch(jobId),
    { jobId },
  );
}

export async function pauseBatch(jobId: string) {
  return apiClient.request('pause_batch', () =>
    apiClient.getMockProvider().pauseBatch(jobId),
    { jobId },
  );
}

export async function resumeBatch(jobId: string) {
  return apiClient.request('resume_batch', () =>
    apiClient.getMockProvider().resumeBatch(jobId),
    { jobId },
  );
}

export async function cancelBatch(jobId: string) {
  return apiClient.request('cancel_batch', () =>
    apiClient.getMockProvider().cancelBatch(jobId),
    { jobId },
  );
}

export async function getBatchJob(jobId: string): Promise<BatchJob | null> {
  return apiClient.request('get_batch_job', () =>
    apiClient.getMockProvider().getBatchJob(jobId),
    { jobId },
  );
}

export async function listBatchJobs(): Promise<BatchJob[]> {
  return apiClient.request('list_batch_jobs', () =>
    apiClient.getMockProvider().listBatchJobs(),
  );
}
