import { apiClient } from './client';

export async function getJobsSnapshot() {
  return apiClient.request('get_jobs_snapshot', () => apiClient.getMockProvider().getJobsSnapshot());
}

export async function getWarnings() {
  return apiClient.request('get_warnings', () => apiClient.getMockProvider().getWarnings());
}

export async function getTraceItems() {
  return apiClient.request('get_trace_items', () => apiClient.getMockProvider().getTraceItems());
}
