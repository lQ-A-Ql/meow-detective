import { apiClient } from './client';
import type { ExportOptions, ExportScope } from '@/types/models';

export async function getReportTemplates() {
  return apiClient.request('get_report_templates', () => apiClient.getMockProvider().getReportTemplates());
}

export async function getReportHistory() {
  return apiClient.request('get_report_history', () => apiClient.getMockProvider().getReportHistory());
}

export async function exportHtmlReport(scope?: ExportScope, options?: ExportOptions) {
  return apiClient.request('export_html_report', () => Promise.resolve('Export not available in mock mode'), {
    scope: scope ? { ...scope, overwrite: options?.overwrite ?? false } : undefined,
  });
}

export async function exportCsvReport(scope?: ExportScope, options?: ExportOptions) {
  return apiClient.request('export_csv_report', () => Promise.resolve('Export not available in mock mode'), {
    scope: scope ? { ...scope, overwrite: options?.overwrite ?? false } : undefined,
  });
}

export async function exportJsonReport(scope?: ExportScope, options?: ExportOptions) {
  return apiClient.request('export_json_report', () => Promise.resolve('Export not available in mock mode'), {
    scope: scope ? { ...scope, overwrite: options?.overwrite ?? false } : undefined,
  });
}

export async function exportCsvCorrelationReport(scope?: ExportScope, options?: ExportOptions) {
  return apiClient.request('export_csv_correlation_report', () => Promise.resolve('Export not available in mock mode'), {
    scope: scope ? { ...scope, overwrite: options?.overwrite ?? false } : undefined,
  });
}
