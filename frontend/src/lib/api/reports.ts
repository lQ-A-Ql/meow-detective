import { apiClient } from './client';
import type { ExportOptions, ExportScope, ReportHistoryItem, ReportTemplate } from '@/types/models';

export async function getReportTemplates(): Promise<ReportTemplate[]> {
  return apiClient.request('get_report_templates');
}

export async function getReportHistory(): Promise<ReportHistoryItem[]> {
  return apiClient.request('get_report_history');
}

export async function exportHtmlReport(scope?: ExportScope, options?: ExportOptions): Promise<string> {
  return apiClient.request('export_html_report', {
    scope: scope ? { ...scope, overwrite: options?.overwrite ?? false } : undefined,
  });
}

export async function exportCsvReport(scope?: ExportScope, options?: ExportOptions): Promise<string> {
  return apiClient.request('export_csv_report', {
    scope: scope ? { ...scope, overwrite: options?.overwrite ?? false } : undefined,
  });
}

export async function exportJsonReport(scope?: ExportScope, options?: ExportOptions): Promise<string> {
  return apiClient.request('export_json_report', {
    scope: scope ? { ...scope, overwrite: options?.overwrite ?? false } : undefined,
  });
}

export async function exportCsvCorrelationReport(scope?: ExportScope, options?: ExportOptions): Promise<string> {
  return apiClient.request('export_csv_correlation_report', {
    scope: scope ? { ...scope, overwrite: options?.overwrite ?? false } : undefined,
  });
}
