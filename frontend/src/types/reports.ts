export interface ReportTemplate {
  id: string;
  name: string;
  description: string;
}

export interface ReportHistoryItem {
  id: string;
  fileName: string;
  createdBy: string;
  createdAt: string;
  status: 'completed' | 'running';
  progress?: number;
}

export interface ExportScope {
  fileSystemMetadata: boolean;
  registry: boolean;
  fullTimeline: boolean;
  rawFileExtraction: boolean;
}

export interface ExportOptions {
  overwrite?: boolean;
}
