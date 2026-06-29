export interface ApiErrorDto {
  code: string;
  message: string;
  category?: ErrorCategory;
  details?: unknown;
  recoverable?: boolean;
  suggestion?: string;
}

export type ErrorCategory =
  | 'validation'
  | 'unsupported'
  | 'io'
  | 'parser'
  | 'security'
  | 'external'
  | 'timeout'
  | 'internal';

export interface AppSettings {
  caseRoot: string;
  imageSearchPaths: string[];
  devEventTrace: boolean;
  maxImportWorkers?: number;
  maxAnalysisWorkers?: number;
  importAnalysisMode?: 'metadataOnly' | 'budgetedContent' | 'fullContent';
  hexChunkBytes?: number;
  maxViewerRangeLength?: number;
  maxInlineImagePreviewBytes?: number;
  maxInlineMediaPreviewBytes?: number;
}
