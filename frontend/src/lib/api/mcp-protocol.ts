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
  permissions?: McpPermissionProfile;
}

export interface McpPermissionProfile {
  resourceAccess: 'readOnly' | 'disabled';
  toolAccess: 'allowAll' | 'disabled' | 'allowList';
  promptAccess: 'readOnly' | 'disabled';
  networkPolicy: 'localhostOnly' | 'privateLanAllowed' | 'anyHost';
  allowedTools: string[];
  deniedTools: string[];
  allowedCommands: string[];
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
  permissions: McpPermissionProfile;
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

export interface McpCapabilities {
  resources: boolean;
  tools: boolean;
  prompts: boolean;
}

export interface McpTestConnectionResponse {
  success: boolean;
  error?: string;
  capabilities?: McpCapabilities;
}

export interface McpToolCallResponse {
  success: boolean;
  data?: unknown;
  error?: string;
}

export interface McpServerProtocolDto {
  id: string;
  name: string;
  transport_type: string;
  url?: string;
  command?: string;
  args?: string[];
  enabled: boolean;
  auto_connect: boolean;
  permissions?: {
    resource_access?: unknown;
    tool_access?: unknown;
    prompt_access?: unknown;
    network_policy?: unknown;
    allowed_tools?: unknown;
    denied_tools?: unknown;
    allowed_commands?: unknown;
  };
}

export interface McpConfigProtocolDto {
  servers: unknown;
  resources?: unknown;
  tools?: unknown;
}

export interface McpServerStatusProtocolDto {
  id?: unknown;
  name?: unknown;
  connected?: unknown;
  has_resources?: unknown;
  has_tools?: unknown;
  has_prompts?: unknown;
  last_error?: unknown;
}

export interface McpResourceProtocolDto {
  uri?: unknown;
  name?: unknown;
  description?: unknown;
  mime_type?: unknown;
}

export interface McpToolProtocolDto {
  name?: unknown;
  description?: unknown;
  input_schema?: unknown;
}

export interface McpPromptProtocolDto {
  name?: unknown;
  description?: unknown;
  arguments?: unknown;
}

export interface McpPromptArgumentProtocolDto {
  name?: unknown;
  description?: unknown;
  required?: unknown;
}

export interface McpTestConnectionProtocolDto {
  success?: unknown;
  error?: unknown;
  capabilities?: unknown;
}

export interface McpToolCallProtocolDto {
  success?: unknown;
  data?: unknown;
  error?: unknown;
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
    Object.entries(value).filter((entry): entry is [string, boolean] => typeof entry[1] === 'boolean'),
  );
}

function normalizeStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === 'string') : [];
}

export function normalizePermissions(value: unknown): McpPermissionProfile {
  if (!isRecord(value)) {
    return {
      resourceAccess: 'readOnly', toolAccess: 'allowAll', promptAccess: 'readOnly',
      networkPolicy: 'localhostOnly', allowedTools: [], deniedTools: [], allowedCommands: [],
    };
  }
  return {
    resourceAccess: value.resource_access === 'disabled' ? 'disabled' : 'readOnly',
    toolAccess: value.tool_access === 'allowList' ? 'allowList' : value.tool_access === 'disabled' ? 'disabled' : 'allowAll',
    promptAccess: value.prompt_access === 'disabled' ? 'disabled' : 'readOnly',
    networkPolicy: value.network_policy === 'privateLanAllowed' ? 'privateLanAllowed' : value.network_policy === 'anyHost' ? 'anyHost' : 'localhostOnly',
    allowedTools: normalizeStringArray(value.allowed_tools),
    deniedTools: normalizeStringArray(value.denied_tools),
    allowedCommands: normalizeStringArray(value.allowed_commands),
  };
}

export function toProtocolServerConfig(server: McpServerConfigInput): McpServerProtocolDto {
  const permissions = server.permissions;
  return {
    id: server.id, name: server.name, transport_type: server.transportType, url: server.url,
    command: server.command, args: server.args, enabled: server.enabled, auto_connect: server.autoConnect,
    permissions: {
      resource_access: permissions?.resourceAccess ?? 'readOnly', tool_access: permissions?.toolAccess ?? 'allowAll',
      prompt_access: permissions?.promptAccess ?? 'readOnly', network_policy: permissions?.networkPolicy ?? 'localhostOnly',
      allowed_tools: permissions?.allowedTools ?? [], denied_tools: permissions?.deniedTools ?? [],
      allowed_commands: permissions?.allowedCommands ?? [],
    },
  };
}

function normalizeTransportType(value: unknown): McpTransportType {
  return value === 'stdio' ? 'stdio' : 'sse';
}

export function normalizeServer(value: unknown): McpServer | null {
  if (!isRecord(value)) return null;
  return {
    id: optionalString(value.id) ?? '', name: optionalString(value.name) ?? '',
    transportType: normalizeTransportType(value.transport_type), url: optionalString(value.url),
    command: optionalString(value.command),
    args: Array.isArray(value.args) ? value.args.filter((arg): arg is string => typeof arg === 'string') : undefined,
    enabled: value.enabled === true, autoConnect: value.auto_connect === true,
    permissions: normalizePermissions(value.permissions),
  };
}

export function normalizeConfig(value: unknown): McpConfig {
  if (!isRecord(value)) return { servers: [], resources: {}, tools: {} };
  return {
    servers: Array.isArray(value.servers) ? value.servers.map(normalizeServer).filter((server): server is McpServer => server !== null) : [],
    resources: booleanRecord(value.resources), tools: booleanRecord(value.tools),
  };
}

export function normalizeStatus(value: unknown): McpServerStatus {
  if (!isRecord(value)) return { id: '', name: '', connected: false, hasResources: false, hasTools: false, hasPrompts: false };
  return {
    id: optionalString(value.id) ?? '', name: optionalString(value.name) ?? '', connected: value.connected === true,
    hasResources: value.has_resources === true, hasTools: value.has_tools === true,
    hasPrompts: value.has_prompts === true, lastError: optionalString(value.last_error),
  };
}

export function normalizeResource(value: McpResourceProtocolDto): McpResource {
  return { uri: optionalString(value.uri) ?? '', name: optionalString(value.name) ?? '', description: optionalString(value.description), mimeType: optionalString(value.mime_type) };
}

function isJsonSchema(value: unknown): value is JsonSchema {
  return isRecord(value) && typeof value.type === 'string';
}

export function normalizeTool(value: McpToolProtocolDto): McpTool {
  return { name: optionalString(value.name) ?? '', description: optionalString(value.description) ?? '', inputSchema: isJsonSchema(value.input_schema) ? value.input_schema : { type: 'object' } };
}

function normalizePromptArgument(value: unknown): McpPromptArgument | null {
  if (!isRecord(value)) return null;
  return { name: optionalString(value.name) ?? '', description: optionalString(value.description), required: value.required === true };
}

export function normalizePrompt(value: McpPromptProtocolDto): McpPrompt {
  return {
    name: optionalString(value.name) ?? '', description: optionalString(value.description),
    arguments: Array.isArray(value.arguments) ? value.arguments.map(normalizePromptArgument).filter((argument): argument is McpPromptArgument => argument !== null) : [],
  };
}

export function normalizeList<T>(value: unknown, mapper: (entry: Record<string, unknown>) => T): T[] {
  return Array.isArray(value) ? value.filter(isRecord).map(mapper) : [];
}

function normalizeCapabilities(value: unknown): McpCapabilities | undefined {
  if (!isRecord(value)) return undefined;
  return { resources: value.resources === true, tools: value.tools === true, prompts: value.prompts === true };
}

export function normalizeTestConnection(value: unknown): McpTestConnectionResponse {
  if (!isRecord(value)) return { success: false };
  return { success: value.success === true, error: optionalString(value.error), capabilities: normalizeCapabilities(value.capabilities) };
}

export function normalizeToolCall(value: unknown): McpToolCallResponse {
  if (!isRecord(value)) return { success: false };
  return { success: value.success === true, data: value.data, error: optionalString(value.error) };
}
