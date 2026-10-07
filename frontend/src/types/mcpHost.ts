import type { JsonSchema } from '@/lib/api/mcp';

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
