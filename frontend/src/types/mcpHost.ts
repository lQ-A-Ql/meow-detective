export interface JsonSchema {
  type: string;
  properties?: Record<string, JsonSchema>;
  required?: string[];
  items?: JsonSchema;
  description?: string;
}

export interface McpHostSettings {
  enabled: boolean;
  disabledTools: string[];
  policyVersion?: number;
}

export interface McpHostStatus {
  settings: McpHostSettings;
  running: boolean;
  endpoint: string;
  lastError?: string;
}

export interface McpHostTool {
  name: string;
  description: string;
  inputSchema: JsonSchema;
  outputSchema: JsonSchema;
  readOnly: boolean;
  destructive: boolean;
}

export interface McpHostToolCallResult {
  success: boolean;
  data?: unknown;
  error?: string;
  errorDetails?: import('./common').ApiErrorDto;
}

export interface McpHostMetrics {
  running: boolean;
  activeRequests: number;
  queuedRequests: number;
  taskOwnerCount: number;
  totalCalls: number;
  successfulCalls: number;
  failedCalls: number;
  cancelledCalls: number;
  totalDurationMs: number;
  averageDurationMs: number;
  currentRssMb: number;
  peakRssMb: number;
}

export interface McpHostListFilesRequest {
  dataSourceId: string;
  parentId?: string;
  offset?: number;
  limit?: number;
}

export interface McpHostFileRequest {
  fileId: string;
}

export type McpHostFileEncoding = 'auto' | 'utf8' | 'base64';

export interface McpHostReadFileRequest extends McpHostFileRequest {
  offset?: number;
  length?: number;
  encoding?: McpHostFileEncoding;
}

export interface McpHostExtractFileRequest extends McpHostFileRequest {
  destinationPath?: string;
  destinationGrantId?: string;
  relativePath?: string;
  overwrite?: boolean;
}

export interface McpHostFileChunk {
  fileId: string;
  size: number;
  offset: number;
  bytesRead: number;
  nextOffset: number;
  eof: boolean;
  encoding: McpHostFileEncoding;
  content: string;
  chunkSha256: string;
}
