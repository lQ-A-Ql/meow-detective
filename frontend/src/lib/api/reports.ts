import { apiClient } from './client';

export async function getReportTemplates() {
  return apiClient.request('get_report_templates', () => apiClient.getMockProvider().getReportTemplates());
}

export async function getReportHistory() {
  return apiClient.request('get_report_history', () => apiClient.getMockProvider().getReportHistory());
}
