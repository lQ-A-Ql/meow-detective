export type ReportFormat = 'html' | 'csv' | 'json' | 'markdown';
export type ReportStatus = 'running' | 'completed' | 'failed' | 'cancelled';

export interface ReportScope {
  operatingSystem: boolean;
  hardware: boolean;
  browsers: boolean;
  email: boolean;
}

export interface ExportReportRequest {
  caseId: string;
  format: ReportFormat;
  scope: ReportScope;
  dataSourceId?: string;
}

export interface ReportHistoryItem {
  id: string;
  fileName: string;
  createdBy: string;
  createdAt: string;
  status: ReportStatus;
  progress?: number;
  request: ExportReportRequest;
  errorCode?: string;
  errorMessage?: string;
  fileSize?: number;
  sha256?: string;
}

export interface SaveReportRequest {
  caseId: string;
  reportId: string;
  destination: string;
  overwrite?: boolean;
}
