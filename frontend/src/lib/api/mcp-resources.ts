import { COMMANDS } from './commands';
import { apiClient } from './client';
import {
  normalizeList, normalizePrompt, normalizeResource, normalizeTool, normalizeToolCall,
  type McpToolCallProtocolDto,
} from './mcp-protocol';
export type { McpPrompt, McpResource, McpTool } from './mcp-protocol';

export async function listMcpResources(serverId: string) {
  const response = await apiClient.request<unknown>(COMMANDS.mcp.LIST_MCP_RESOURCES, { serverId });
  return normalizeList(response, normalizeResource);
}

export async function listMcpTools(serverId: string) {
  const response = await apiClient.request<unknown>(COMMANDS.mcp.LIST_MCP_TOOLS, { serverId });
  return normalizeList(response, normalizeTool);
}

export async function callMcpTool(serverId: string, toolName: string, args: unknown) {
  const response = await apiClient.request<McpToolCallProtocolDto>(COMMANDS.mcp.CALL_MCP_TOOL, {
    request: { server_id: serverId, tool_name: toolName, arguments: args },
  });
  return normalizeToolCall(response);
}

export async function listMcpPrompts(serverId: string) {
  const response = await apiClient.request<unknown>(COMMANDS.mcp.LIST_MCP_PROMPTS, { serverId });
  return normalizeList(response, normalizePrompt);
}

export function getMcpPrompt(serverId: string, promptName: string, args?: Record<string, string>) {
  return apiClient.request<string>(COMMANDS.mcp.GET_MCP_PROMPT, { serverId, promptName, arguments: args });
}
