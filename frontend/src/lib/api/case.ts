import { apiClient } from './client';

export async function getCurrentCase() {
  return apiClient.request('get_current_case', () => apiClient.getMockProvider().getCurrentCase());
}

export async function getCaseMetrics() {
  return apiClient.request('get_case_metrics', () => apiClient.getMockProvider().getCaseMetrics());
}

export async function getRecentObjects() {
  return apiClient.request('get_recent_objects', () => apiClient.getMockProvider().getRecentObjects());
}
