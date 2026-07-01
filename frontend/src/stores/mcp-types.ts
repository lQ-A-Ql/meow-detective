import type { McpPermissionProfile } from '@/lib/api/mcp';

export interface McpServer {
  id: string;
  name: string;
  transportType: 'sse' | 'stdio';
  url?: string;
  command?: string;
  args?: string[];
  enabled: boolean;
  autoConnect: boolean;
  permissions: McpPermissionProfile;
  connected: boolean;
  hasResources: boolean;
  hasTools: boolean;
  hasPrompts: boolean;
  lastError?: string;
}

export type NewMcpServerInput = Omit<
  McpServer,
  'id' | 'permissions' | 'connected' | 'hasResources' | 'hasTools' | 'hasPrompts'
> & {
  permissions?: McpPermissionProfile;
};

export interface McpToolCallResult {
  success: boolean;
  data?: unknown;
  error?: string;
}
