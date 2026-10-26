import type { McpHostSettings, McpHostStatus, McpHostTool, McpHostToolCallResult } from '@/types/models';
import { apiClient } from './client';
import { COMMANDS } from './commands';
import type { McpHostClient, McpHostCreatedClient, McpHostCreateClientRequest } from '@/types/mcpHost';

export const listMcpHostClients = () => apiClient.request<McpHostClient[]>(COMMANDS.mcp.LIST_MCP_HOST_CLIENTS);
export const createMcpHostClient = (request: McpHostCreateClientRequest) =>
  apiClient.request<McpHostCreatedClient>(COMMANDS.mcp.CREATE_MCP_HOST_CLIENT, { request });
export const revokeMcpHostClient = (clientId: string) =>
  apiClient.request<void>(COMMANDS.mcp.REVOKE_MCP_HOST_CLIENT, { clientId });

export const getMcpHostStatus = () => apiClient.request<McpHostStatus>(COMMANDS.mcp.GET_MCP_HOST_STATUS);
export const listMcpHostTools = () => apiClient.request<McpHostTool[]>(COMMANDS.mcp.LIST_MCP_HOST_TOOLS);
export const setMcpHostSettings = (settings: McpHostSettings) =>
  apiClient.request<McpHostStatus>(COMMANDS.mcp.SET_MCP_HOST_SETTINGS, { settings });
export const callMcpHostTool = (name: string, argumentsValue: unknown) =>
  apiClient.request<McpHostToolCallResult>(COMMANDS.mcp.CALL_MCP_HOST_TOOL, { request: { name, arguments: argumentsValue } });
