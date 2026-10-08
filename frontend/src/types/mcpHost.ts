import type { JsonSchema } from '@/lib/api/mcp-protocol';

export interface McpHostSettings {
  enabled: boolean;
  disabledTools: string[];
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
}

export interface McpHostToolCallResult {
  success: boolean;
  data?: unknown;
  error?: string;
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
