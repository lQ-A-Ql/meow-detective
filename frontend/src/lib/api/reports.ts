import { apiClient } from './client';

export async function getReportTemplates() {
  return apiClient.request('get_report_templates', () => apiClient.getMockProvider().getReportTemplates());
}

export async function getReportHistory() {
  return apiClient.request('get_report_history', () => apiClient.getMockProvider().getReportHistory());
}

export async function exportHtmlReport() {
  return apiClient.request('export_html_report', () => Promise.resolve('Export not available in mock mode'));
}

export async function exportCsvReport() {
  return apiClient.request('export_csv_report', () => Promise.resolve('Export not available in mock mode'));
}

export async function exportJsonReport() {
  return apiClient.request('export_json_report', () => Promise.resolve('Export not available in mock mode'));
}
