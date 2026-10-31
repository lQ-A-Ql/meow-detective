import type { McpHostSettings, McpHostStatus, McpHostTool, McpHostToolCallResult } from '@/types/models';
import { apiClient } from './client';
import { COMMANDS } from './commands';

export const getMcpHostStatus = () => apiClient.request<McpHostStatus>(COMMANDS.mcp.GET_MCP_HOST_STATUS);
export const listMcpHostTools = () => apiClient.request<McpHostTool[]>(COMMANDS.mcp.LIST_MCP_HOST_TOOLS);
export const setMcpHostSettings = (settings: McpHostSettings) =>
  apiClient.request<McpHostStatus>(COMMANDS.mcp.SET_MCP_HOST_SETTINGS, { settings });
export const callMcpHostTool = (name: string, argumentsValue: unknown) =>
  apiClient.request<McpHostToolCallResult>(COMMANDS.mcp.CALL_MCP_HOST_TOOL, { request: { name, arguments: argumentsValue } });
