import type { BatchJob, BatchPlanInput } from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function createBatchPlan(plan: BatchPlanInput): Promise<BatchJob> {
  return apiClient.request(COMMANDS.batch.CREATE_BATCH_PLAN, {
    name: plan.name,
    data_source_ids: plan.dataSourceIds,
    phases: plan.phases,
    resource_limits: {
      maxMemoryMb: plan.resourceLimits.maxMemoryMb,
      maxThreads: plan.resourceLimits.maxThreads,
    },
  });
}

export async function startBatch(jobId: string) {
  return apiClient.request(COMMANDS.batch.START_BATCH, { batch_id: jobId });
}

export async function pauseBatch(jobId: string) {
  return apiClient.request(COMMANDS.batch.PAUSE_BATCH, { batch_id: jobId });
}

export async function resumeBatch(jobId: string) {
  return apiClient.request(COMMANDS.batch.RESUME_BATCH, { batch_id: jobId });
}

export async function cancelBatch(jobId: string) {
  return apiClient.request(COMMANDS.batch.CANCEL_BATCH, { batch_id: jobId });
}

export async function getBatchJob(jobId: string): Promise<BatchJob | null> {
  return apiClient.request(COMMANDS.batch.GET_BATCH_JOB, { batch_id: jobId });
}

export async function listBatchJobs(): Promise<BatchJob[]> {
  return apiClient.request(COMMANDS.batch.LIST_BATCH_JOBS);
}
