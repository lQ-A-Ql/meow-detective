import fixture from './mcp.json';

if (import.meta.env.PROD) throw new Error('MCP mock is development-only');

let config = structuredClone(fixture.config);
let host = structuredClone(fixture.host);
let nextServer = 1;
const connected = new Set<string>();
const commands = new Set([
  'get_mcp_config', 'save_mcp_config', 'get_mcp_host_status', 'set_mcp_host_settings', 'list_mcp_host_tools',
  'call_mcp_host_tool', 'add_mcp_server', 'remove_mcp_server', 'connect_mcp_server', 'disconnect_mcp_server',
  'test_mcp_connection', 'list_mcp_resources', 'list_mcp_tools', 'call_mcp_tool', 'list_mcp_prompts',
]);

export const handlesMcpCommand = (command: string) => commands.has(command);

function serverStatus(id: string) {
  const server = config.servers.find((item) => item.id === id);
  return { id, name: server?.name ?? '', connected: connected.has(id), has_resources: true, has_tools: true, has_prompts: false };
}

/** Synthetic MCP lifecycle, never opens a socket or launches a command. */
export function invokeMcpCommand(command: string, payload: Record<string, unknown> = {}): unknown {
  switch (command) {
    case 'get_mcp_config': return structuredClone(config);
    case 'save_mcp_config': config = structuredClone(payload.config as typeof config); return null;
    case 'get_mcp_host_status': return structuredClone(host);
    case 'set_mcp_host_settings': {
      host = { ...host, settings: structuredClone(payload.settings as typeof host.settings) };
      host.running = host.settings.enabled;
      return structuredClone(host);
    }
    case 'list_mcp_host_tools': return fixture.tools;
    case 'add_mcp_server': {
      const server = { ...(payload.server as typeof config.servers[number]), id: `mcp-preview-${nextServer++}` };
      config.servers.push(server);
      return serverStatus(server.id);
    }
    case 'remove_mcp_server': config.servers = config.servers.filter((item) => item.id !== payload.serverId); return null;
    case 'connect_mcp_server': connected.add(String(payload.serverId)); return serverStatus(String(payload.serverId));
    case 'disconnect_mcp_server': connected.delete(String(payload.serverId)); return null;
    case 'test_mcp_connection': return { success: true, capabilities: { resources: true, tools: true, prompts: false } };
    case 'list_mcp_resources': return fixture.resources;
    case 'list_mcp_tools': return fixture.tools.map((tool) => ({ name: tool.name, description: tool.description, input_schema: tool.inputSchema }));
    case 'list_mcp_prompts': return [];
    case 'call_mcp_host_tool':
    case 'call_mcp_tool': {
      const request = payload.request as { name?: string; tool_name?: string };
      return { success: true, data: { mock: true, tool: request.name ?? request.tool_name, caseId: 'case-mock-001', sourceCount: 3 } };
    }
    default: return undefined;
  }
}
