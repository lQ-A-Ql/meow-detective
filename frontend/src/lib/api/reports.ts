import { COMMANDS } from './commands';
import { apiClient } from './client';
import type { ExportReportRequest, ReportHistoryItem, SaveReportRequest } from '@/types/models';

export function getReportHistory(caseId: string): Promise<ReportHistoryItem[]> {
  return apiClient.request(COMMANDS.reports.GET_REPORT_HISTORY, { caseId });
}

export function exportReport(request: ExportReportRequest): Promise<ReportHistoryItem> {
  return apiClient.request(COMMANDS.reports.EXPORT_REPORT, { request });
}

export function saveReport(request: SaveReportRequest): Promise<void> {
  return apiClient.request(COMMANDS.reports.SAVE_REPORT, { request });
}
