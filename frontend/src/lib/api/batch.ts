import type { BatchJob, BatchPlan } from '@/types/models';
import { apiClient } from './client';

export async function createBatchPlan(plan: BatchPlan) {
  return apiClient.request('create_batch_plan', plan as unknown as Record<string, unknown>);
}

export async function startBatch(jobId: string) {
  return apiClient.request('start_batch', { jobId });
}

export async function pauseBatch(jobId: string) {
  return apiClient.request('pause_batch', { jobId });
}

export async function resumeBatch(jobId: string) {
  return apiClient.request('resume_batch', { jobId });
}

export async function cancelBatch(jobId: string) {
  return apiClient.request('cancel_batch', { jobId });
}

export async function getBatchJob(jobId: string): Promise<BatchJob | null> {
  return apiClient.request('get_batch_job', { jobId });
}

export async function listBatchJobs(): Promise<BatchJob[]> {
  return apiClient.request('list_batch_jobs');
}
