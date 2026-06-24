import { JobSnapshot, TraceItem, WarningItem } from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function getJobsSnapshot(): Promise<JobSnapshot[]> {
  return apiClient.request(COMMANDS.jobs.GET_JOBS_SNAPSHOT);
}

export async function getWarnings(): Promise<WarningItem[]> {
  return apiClient.request(COMMANDS.jobs.GET_WARNINGS);
}

export async function getTraceItems(): Promise<TraceItem[]> {
  return apiClient.request(COMMANDS.jobs.GET_TRACE_ITEMS);
}
