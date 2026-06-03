import { apiClient } from './client';
import type { ExportScope } from '@/types/models';

export async function getReportTemplates() {
  return apiClient.request('get_report_templates', () => apiClient.getMockProvider().getReportTemplates());
}

export async function getReportHistory() {
  return apiClient.request('get_report_history', () => apiClient.getMockProvider().getReportHistory());
}

export async function exportHtmlReport(scope?: ExportScope) {
  return apiClient.request('export_html_report', () => Promise.resolve('Export not available in mock mode'), { scope });
}

export async function exportCsvReport(scope?: ExportScope) {
  return apiClient.request('export_csv_report', () => Promise.resolve('Export not available in mock mode'), { scope });
}

export async function exportJsonReport(scope?: ExportScope) {
  return apiClient.request('export_json_report', () => Promise.resolve('Export not available in mock mode'), { scope });
}
