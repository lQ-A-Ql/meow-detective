import { COMMANDS } from './commands';
import { apiClient } from './client';
import type { ExportOptions, ExportScope, ReportHistoryItem, ReportTemplate } from '@/types/models';

export async function getReportTemplates(): Promise<ReportTemplate[]> {
  return apiClient.request(COMMANDS.reports.GET_REPORT_TEMPLATES);
}

export async function getReportHistory(): Promise<ReportHistoryItem[]> {
  return apiClient.request(COMMANDS.reports.GET_REPORT_HISTORY);
}

export async function exportHtmlReport(scope?: ExportScope, options?: ExportOptions): Promise<string> {
  return apiClient.request(COMMANDS.reports.EXPORT_HTML_REPORT, {
    scope: scope ? { ...scope, overwrite: options?.overwrite ?? false } : undefined,
  });
}

export async function exportCsvReport(scope?: ExportScope, options?: ExportOptions): Promise<string> {
  return apiClient.request(COMMANDS.reports.EXPORT_CSV_REPORT, {
    scope: scope ? { ...scope, overwrite: options?.overwrite ?? false } : undefined,
  });
}

export async function exportJsonReport(scope?: ExportScope, options?: ExportOptions): Promise<string> {
  return apiClient.request(COMMANDS.reports.EXPORT_JSON_REPORT, {
    scope: scope ? { ...scope, overwrite: options?.overwrite ?? false } : undefined,
  });
}

export async function exportCsvCorrelationReport(scope?: ExportScope, options?: ExportOptions): Promise<string> {
  return apiClient.request(COMMANDS.reports.EXPORT_CSV_CORRELATION_REPORT, {
    scope: scope ? { ...scope, overwrite: options?.overwrite ?? false } : undefined,
  });
}
