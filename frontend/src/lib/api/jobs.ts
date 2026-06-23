import { JobSnapshot, TraceItem, WarningItem } from '@/types/models';
import { apiClient } from './client';

export async function getJobsSnapshot(): Promise<JobSnapshot[]> {
  return apiClient.request('get_jobs_snapshot');
}

export async function getWarnings(): Promise<WarningItem[]> {
  return apiClient.request('get_warnings');
}

export async function getTraceItems(): Promise<TraceItem[]> {
  return apiClient.request('get_trace_items');
}
