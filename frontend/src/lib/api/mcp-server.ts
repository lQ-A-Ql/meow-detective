import { COMMANDS } from './commands';
import { apiClient } from './client';
import {
  normalizeConfig, normalizeStatus, normalizeTestConnection, toProtocolServerConfig,
  type McpConfigProtocolDto, type McpServerStatusProtocolDto, type McpTestConnectionProtocolDto,
  type McpServerConfigInput, type McpPermissionProfile,
} from './mcp-protocol';
export type { McpConfig, McpPermissionProfile, McpServer, McpServerStatus } from './mcp-protocol';

export async function getMcpConfig() {
  const response = await apiClient.request<McpConfigProtocolDto>(COMMANDS.mcp.GET_MCP_CONFIG);
  return normalizeConfig(response);
}

export function saveMcpConfig(servers: McpServerConfigInput[]) {
  return apiClient.request(COMMANDS.mcp.SAVE_MCP_CONFIG, {
    config: { servers: servers.map(toProtocolServerConfig), resources: {}, tools: {} },
  });
}

export async function addMcpServer(server: McpServerConfigInput) {
  const response = await apiClient.request<McpServerStatusProtocolDto>(COMMANDS.mcp.ADD_MCP_SERVER, { server: toProtocolServerConfig(server) });
  return normalizeStatus(response);
}

export function removeMcpServer(serverId: string) {
  return apiClient.request(COMMANDS.mcp.REMOVE_MCP_SERVER, { serverId });
}

export async function connectMcpServer(serverId: string) {
  const response = await apiClient.request<McpServerStatusProtocolDto>(COMMANDS.mcp.CONNECT_MCP_SERVER, { serverId });
  return normalizeStatus(response);
}

export function disconnectMcpServer(serverId: string) {
  return apiClient.request(COMMANDS.mcp.DISCONNECT_MCP_SERVER, { serverId });
}

export async function testMcpConnection(
  transportType: string, url?: string, command?: string, args?: string[], permissions?: McpPermissionProfile,
) {
  const response = await apiClient.request<McpTestConnectionProtocolDto>(COMMANDS.mcp.TEST_MCP_CONNECTION, {
    request: {
      transport_type: transportType, url, command, args,
      permissions: {
        resource_access: permissions?.resourceAccess ?? 'readOnly', tool_access: permissions?.toolAccess ?? 'allowAll',
        prompt_access: permissions?.promptAccess ?? 'readOnly', network_policy: permissions?.networkPolicy ?? 'localhostOnly',
        allowed_tools: permissions?.allowedTools ?? [], denied_tools: permissions?.deniedTools ?? [],
        allowed_commands: permissions?.allowedCommands ?? [],
      },
    },
  });
  return normalizeTestConnection(response);
}
