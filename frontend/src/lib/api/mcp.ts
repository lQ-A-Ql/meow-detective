import { apiClient } from './client';

export interface JsonSchema {
  type: string;
  properties?: Record<string, JsonSchema>;
  required?: string[];
  items?: JsonSchema;
  description?: string;
}

export type McpTransportType = 'sse' | 'stdio';

export interface McpServerConfigInput {
  id: string;
  name: string;
  transportType: McpTransportType;
  url?: string;
  command?: string;
  args?: string[];
  enabled: boolean;
  autoConnect: boolean;
}

interface McpServerProtocolDto {
  id: string;
  name: string;
  transport_type: string;
  url?: string;
  command?: string;
  args?: string[];
  enabled: boolean;
  auto_connect: boolean;
}

interface McpConfigProtocolDto {
  servers: unknown;
  resources?: unknown;
  tools?: unknown;
}

interface McpServerStatusProtocolDto {
  id?: unknown;
  name?: unknown;
  connected?: unknown;
  has_resources?: unknown;
  has_tools?: unknown;
  has_prompts?: unknown;
  last_error?: unknown;
}

interface McpResourceProtocolDto {
  uri?: unknown;
  name?: unknown;
  description?: unknown;
  mime_type?: unknown;
}

interface McpToolProtocolDto {
  name?: unknown;
  description?: unknown;
  input_schema?: unknown;
}

interface McpPromptProtocolDto {
  name?: unknown;
  description?: unknown;
  arguments?: unknown;
}

interface McpPromptArgumentProtocolDto {
  name?: unknown;
  description?: unknown;
  required?: unknown;
}

interface McpTestConnectionProtocolDto {
  success?: unknown;
  error?: unknown;
}

interface McpToolCallProtocolDto {
  success?: unknown;
  data?: unknown;
  error?: unknown;
}

export interface McpServer {
  id: string;
  name: string;
  transportType: McpTransportType;
  url?: string;
  command?: string;
  args?: string[];
  enabled: boolean;
  autoConnect: boolean;
}

export interface McpConfig {
  servers: McpServer[];
  resources: Record<string, boolean>;
  tools: Record<string, boolean>;
}

export interface McpServerStatus {
  id: string;
  name: string;
  connected: boolean;
  hasResources: boolean;
  hasTools: boolean;
  hasPrompts: boolean;
  lastError?: string;
}

export interface McpResource {
  uri: string;
  name: string;
  description?: string;
  mimeType?: string;
}

export interface McpTool {
  name: string;
  description: string;
  inputSchema: JsonSchema;
}

export interface McpPromptArgument {
  name: string;
  description?: string;
  required: boolean;
}

export interface McpPrompt {
  name: string;
  description?: string;
  arguments: McpPromptArgument[];
}

export interface McpTestConnectionResponse {
  success: boolean;
  error?: string;
}

export interface McpToolCallResponse {
  success: boolean;
  data?: unknown;
  error?: string;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function optionalString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

function booleanRecord(value: unknown): Record<string, boolean> {
  if (!isRecord(value)) return {};

  return Object.fromEntries(
    Object.entries(value).filter((entry): entry is [string, boolean] => typeof entry[1] === 'boolean')
  );
}

function toProtocolServerConfig(server: McpServerConfigInput): McpServerProtocolDto {
  return {
    id: server.id,
    name: server.name,
    transport_type: server.transportType,
    url: server.url,
    command: server.command,
    args: server.args,
    enabled: server.enabled,
    auto_connect: server.autoConnect,
  };
}

function normalizeTransportType(value: unknown): McpTransportType {
  return value === 'stdio' ? 'stdio' : 'sse';
}

function normalizeServer(value: unknown): McpServer | null {
  if (!isRecord(value)) return null;

  return {
    id: optionalString(value.id) ?? '',
    name: optionalString(value.name) ?? '',
    transportType: normalizeTransportType(value.transport_type),
    url: optionalString(value.url),
    command: optionalString(value.command),
    args: Array.isArray(value.args) ? value.args.filter((arg): arg is string => typeof arg === 'string') : undefined,
    enabled: typeof value.enabled === 'boolean' ? value.enabled : false,
    autoConnect: typeof value.auto_connect === 'boolean' ? value.auto_connect : false,
  };
}

function normalizeConfig(value: unknown): McpConfig {
  if (!isRecord(value)) return { servers: [], resources: {}, tools: {} };

  return {
    servers: Array.isArray(value.servers) ? value.servers.map(normalizeServer).filter((server): server is McpServer => server !== null) : [],
    resources: booleanRecord(value.resources),
    tools: booleanRecord(value.tools),
  };
}

function normalizeStatus(value: unknown): McpServerStatus {
  if (!isRecord(value)) {
    return { id: '', name: '', connected: false, hasResources: false, hasTools: false, hasPrompts: false };
  }

  return {
    id: optionalString(value.id) ?? '',
    name: optionalString(value.name) ?? '',
    connected: value.connected === true,
    hasResources: value.has_resources === true,
    hasTools: value.has_tools === true,
    hasPrompts: value.has_prompts === true,
    lastError: optionalString(value.last_error),
  };
}

function normalizeResource(value: McpResourceProtocolDto): McpResource {
  return {
    uri: optionalString(value.uri) ?? '',
    name: optionalString(value.name) ?? '',
    description: optionalString(value.description),
    mimeType: optionalString(value.mime_type),
  };
}

function normalizeTool(value: McpToolProtocolDto): McpTool {
  return {
    name: optionalString(value.name) ?? '',
    description: optionalString(value.description) ?? '',
    inputSchema: isRecord(value.input_schema) ? (value.input_schema as unknown as JsonSchema) : { type: 'object' },
  };
}

function normalizePromptArgument(value: unknown): McpPromptArgument | null {
  if (!isPromptArgumentProtocolDto(value)) return null;

  return {
    name: optionalString(value.name) ?? '',
    description: optionalString(value.description),
    required: value.required === true,
  };
}

function isPromptArgumentProtocolDto(value: unknown): value is McpPromptArgumentProtocolDto {
  return isRecord(value);
}

function normalizePrompt(value: McpPromptProtocolDto): McpPrompt {
  return {
    name: optionalString(value.name) ?? '',
    description: optionalString(value.description),
    arguments: Array.isArray(value.arguments)
      ? value.arguments.map(normalizePromptArgument).filter((argument): argument is McpPromptArgument => argument !== null)
      : [],
  };
}

function normalizeList<T>(value: unknown, mapper: (entry: Record<string, unknown>) => T): T[] {
  if (!Array.isArray(value)) return [];

  return value.filter(isRecord).map(mapper);
}

function normalizeTestConnection(value: unknown): McpTestConnectionResponse {
  if (!isRecord(value)) return { success: false };

  return {
    success: value.success === true,
    error: optionalString(value.error),
  };
}

function normalizeToolCall(value: unknown): McpToolCallResponse {
  if (!isRecord(value)) return { success: false };

  return {
    success: value.success === true,
    data: value.data,
    error: optionalString(value.error),
  };
}

export async function getMcpConfig() {
  const response = await apiClient.request<McpConfigProtocolDto>(
    'get_mcp_config',
    () => Promise.resolve({ servers: [], resources: {}, tools: {} })
  );
  return normalizeConfig(response);
}

export function saveMcpConfig(servers: McpServerConfigInput[]) {
  return apiClient.request('save_mcp_config', () => Promise.resolve(), {
    config: {
      servers: servers.map(toProtocolServerConfig),
      resources: {},
      tools: {},
    },
  });
}

export async function addMcpServer(server: McpServerConfigInput) {
  const response = await apiClient.request<McpServerStatusProtocolDto>('add_mcp_server', () => Promise.resolve({
    id: server.id,
    name: server.name,
    connected: false,
    has_resources: false,
    has_tools: false,
    has_prompts: false,
  }), { server: toProtocolServerConfig(server) });
  return normalizeStatus(response);
}

export function removeMcpServer(serverId: string) {
  return apiClient.request('remove_mcp_server', () => Promise.resolve(), { serverId });
}

export async function connectMcpServer(serverId: string) {
  const response = await apiClient.request<McpServerStatusProtocolDto>('connect_mcp_server', () => Promise.resolve({
    id: serverId,
    name: '',
    connected: false,
    has_resources: false,
    has_tools: false,
    has_prompts: false,
  }), { serverId });
  return normalizeStatus(response);
}

export function disconnectMcpServer(serverId: string) {
  return apiClient.request('disconnect_mcp_server', () => Promise.resolve(), { serverId });
}

export async function testMcpConnection(transportType: string, url?: string, command?: string, args?: string[]) {
  const response = await apiClient.request<McpTestConnectionProtocolDto>('test_mcp_connection', () => Promise.resolve({ success: false }), {
    request: {
      transport_type: transportType,
      url,
      command,
      args,
    },
  });
  return normalizeTestConnection(response);
}

export async function listMcpResources(serverId: string) {
  const response = await apiClient.request<unknown>('list_mcp_resources', () => Promise.resolve([]), { serverId });
  return normalizeList(response, normalizeResource);
}

export async function listMcpTools(serverId: string) {
  const response = await apiClient.request<unknown>('list_mcp_tools', () => Promise.resolve([]), { serverId });
  return normalizeList(response, normalizeTool);
}

export async function callMcpTool(serverId: string, toolName: string, args: unknown) {
  const response = await apiClient.request<McpToolCallProtocolDto>('call_mcp_tool', () => Promise.resolve({ success: false }), {
    request: {
      server_id: serverId,
      tool_name: toolName,
      arguments: args,
    },
  });
  return normalizeToolCall(response);
}

export async function listMcpPrompts(serverId: string) {
  const response = await apiClient.request<unknown>('list_mcp_prompts', () => Promise.resolve([]), { serverId });
  return normalizeList(response, normalizePrompt);
}

export function getMcpPrompt(serverId: string, promptName: string, args?: Record<string, string>) {
  return apiClient.request<string>('get_mcp_prompt', () => Promise.resolve(''), {
    serverId,
    promptName,
    arguments: args,
  });
}
