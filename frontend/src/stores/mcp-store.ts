import { create } from 'zustand';
import { invoke } from '@tauri-apps/api/core';

// ============================================
// 类型定义
// ============================================

/** JSON Schema 类型 */
interface JsonSchema {
  type: string;
  properties?: Record<string, JsonSchema>;
  required?: string[];
  items?: JsonSchema;
  description?: string;
}

/** MCP Server configuration */
interface McpServer {
  id: string;
  name: string;
  transportType: 'sse' | 'stdio';
  url?: string;
  command?: string;
  args?: string[];
  enabled: boolean;
  autoConnect: boolean;
  connected: boolean;
  hasResources: boolean;
  hasTools: boolean;
  hasPrompts: boolean;
  lastError?: string;
}

/** MCP Resource */
interface McpResource {
  uri: string;
  name: string;
  description?: string;
  mimeType?: string;
}

/** MCP Tool */
interface McpTool {
  name: string;
  description: string;
  inputSchema: JsonSchema;
}

/** MCP Prompt Argument */
interface McpPromptArgument {
  name: string;
  description?: string;
  required: boolean;
}

/** MCP Prompt */
interface McpPrompt {
  name: string;
  description?: string;
  arguments: McpPromptArgument[];
}

/** MCP Tool Call Result */
interface McpToolCallResult {
  success: boolean;
  data?: unknown;
  error?: string;
}

// ============================================
// 后端响应类型
// ============================================

interface McpServerResponse {
  id: string;
  name: string;
  transport_type: string;
  url?: string;
  command?: string;
  args?: string[];
  enabled: boolean;
  auto_connect: boolean;
}

interface McpConfigResponse {
  servers: McpServerResponse[];
  resources: Record<string, boolean>;
  tools: Record<string, boolean>;
}

interface McpServerStatusResponse {
  id: string;
  name: string;
  connected: boolean;
  has_resources: boolean;
  has_tools: boolean;
  has_prompts: boolean;
  last_error?: string;
}

interface McpResourceResponse {
  uri: string;
  name: string;
  description?: string;
  mime_type?: string;
}

interface McpToolResponse {
  name: string;
  description: string;
  input_schema: JsonSchema;
}

interface McpPromptResponse {
  name: string;
  description?: string;
  arguments: Array<{
    name: string;
    description?: string;
    required: boolean;
  }>;
}

interface McpTestConnectionResponse {
  success: boolean;
  error?: string;
}

interface McpToolCallResponse {
  success: boolean;
  data?: unknown;
  error?: string;
}

// ============================================
// Store 状态接口
// ============================================

interface McpState {
  // State
  servers: McpServer[];
  resources: McpResource[];
  tools: McpTool[];
  prompts: McpPrompt[];
  selectedServerId: string | null;
  loading: boolean;
  error: string | null;

  // Config operations
  loadConfig: () => Promise<void>;
  saveConfig: () => Promise<void>;

  // Server operations
  addServer: (server: Omit<McpServer, 'id' | 'connected' | 'hasResources' | 'hasTools' | 'hasPrompts'>) => Promise<void>;
  removeServer: (id: string) => Promise<void>;

  // Connection operations
  connectServer: (id: string) => Promise<void>;
  disconnectServer: (id: string) => Promise<void>;
  testConnection: (transportType: string, url?: string, command?: string, args?: string[]) => Promise<{ success: boolean; error?: string }>;

  // Selection
  selectServer: (id: string | null) => void;

  // Resource operations
  refreshResources: (serverId: string) => Promise<void>;

  // Tool operations
  refreshTools: (serverId: string) => Promise<void>;
  callTool: (serverId: string, toolName: string, args: unknown) => Promise<McpToolCallResult>;

  // Prompt operations
  refreshPrompts: (serverId: string) => Promise<void>;
  getPrompt: (serverId: string, promptName: string, args?: Record<string, string>) => Promise<string>;
}

// ============================================
// 辅助函数
// ============================================

/** 格式化错误信息 */
function formatError(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  if (typeof err === 'object' && err !== null && 'message' in err) {
    return String((err as { message: unknown }).message);
  }
  return '未知错误';
}

/** 转换服务器响应 */
function mapServerResponse(s: McpServerResponse): McpServer {
  return {
    id: s.id,
    name: s.name,
    transportType: s.transport_type as 'sse' | 'stdio',
    url: s.url,
    command: s.command,
    args: s.args,
    enabled: s.enabled,
    autoConnect: s.auto_connect,
    connected: false,
    hasResources: false,
    hasTools: false,
    hasPrompts: false,
  };
}

/** 转换资源响应 */
function mapResourceResponse(r: McpResourceResponse): McpResource {
  return {
    uri: r.uri,
    name: r.name,
    description: r.description,
    mimeType: r.mime_type,
  };
}

/** 转换工具响应 */
function mapToolResponse(t: McpToolResponse): McpTool {
  return {
    name: t.name,
    description: t.description,
    inputSchema: t.input_schema,
  };
}

/** 转换 Prompt 响应 */
function mapPromptResponse(p: McpPromptResponse): McpPrompt {
  return {
    name: p.name,
    description: p.description,
    arguments: p.arguments.map((a) => ({
      name: a.name,
      description: a.description,
      required: a.required,
    })),
  };
}

// ============================================
// Store 实现
// ============================================

export const useMcpStore = create<McpState>((set, get) => ({
  // Initial state
  servers: [],
  resources: [],
  tools: [],
  prompts: [],
  selectedServerId: null,
  loading: false,
  error: null,

  // Load MCP config from backend
  loadConfig: async () => {
    try {
      set({ loading: true, error: null });
      const config = await invoke<McpConfigResponse>('get_mcp_config');

      const servers: McpServer[] = config.servers.map(mapServerResponse);
      set({ servers, loading: false });
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  // Save MCP config to backend
  saveConfig: async () => {
    try {
      const { servers } = get();
      const config = {
        servers: servers.map((s) => ({
          id: s.id,
          name: s.name,
          transport_type: s.transportType,
          url: s.url,
          command: s.command,
          args: s.args,
          enabled: s.enabled,
          auto_connect: s.autoConnect,
        })),
        resources: {},
        tools: {},
      };
      await invoke('save_mcp_config', { config });
    } catch (err) {
      set({ error: formatError(err) });
    }
  },

  // Add a new server
  addServer: async (server) => {
    try {
      set({ loading: true, error: null });
      const id = crypto.randomUUID();

      const serverConfig = {
        id,
        name: server.name,
        transport_type: server.transportType,
        url: server.url,
        command: server.command,
        args: server.args,
        enabled: server.enabled,
        auto_connect: server.autoConnect,
      };

      await invoke('add_mcp_server', { server: serverConfig });
      await get().loadConfig();
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  // Remove a server
  removeServer: async (id) => {
    try {
      set({ loading: true, error: null });
      await invoke('remove_mcp_server', { serverId: id });

      set((state) => ({
        servers: state.servers.filter((s) => s.id !== id),
        selectedServerId: state.selectedServerId === id ? null : state.selectedServerId,
        loading: false,
      }));
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  // Connect to a server
  connectServer: async (id) => {
    try {
      set({ loading: true, error: null });
      const status = await invoke<McpServerStatusResponse>('connect_mcp_server', { serverId: id });

      set((state) => ({
        servers: state.servers.map((s) =>
          s.id === id
            ? {
                ...s,
                connected: status.connected,
                hasResources: status.has_resources,
                hasTools: status.has_tools,
                hasPrompts: status.has_prompts,
                lastError: status.last_error,
              }
            : s
        ),
        loading: false,
      }));
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  // Disconnect from a server
  disconnectServer: async (id) => {
    try {
      set({ loading: true, error: null });
      await invoke('disconnect_mcp_server', { serverId: id });

      set((state) => ({
        servers: state.servers.map((s) =>
          s.id === id ? { ...s, connected: false } : s
        ),
        loading: false,
      }));
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  // Test connection
  testConnection: async (transportType, url, command, args) => {
    try {
      const result = await invoke<McpTestConnectionResponse>('test_mcp_connection', {
        request: {
          transport_type: transportType,
          url,
          command,
          args,
        },
      });
      return {
        success: result.success,
        error: result.error,
      };
    } catch (err) {
      return {
        success: false,
        error: formatError(err),
      };
    }
  },

  // Select a server (合并为单次 set)
  selectServer: (id) => {
    set((state) => {
      if (id === state.selectedServerId) return state;
      return {
        selectedServerId: id,
        resources: [],
        tools: [],
        prompts: [],
      };
    });
  },

  // Refresh resources from a server
  refreshResources: async (serverId) => {
    try {
      set({ loading: true, error: null });
      const resources = await invoke<McpResourceResponse[]>('list_mcp_resources', { serverId });
      set({
        resources: resources.map(mapResourceResponse),
        loading: false,
      });
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  // Refresh tools from a server
  refreshTools: async (serverId) => {
    try {
      set({ loading: true, error: null });
      const tools = await invoke<McpToolResponse[]>('list_mcp_tools', { serverId });
      set({
        tools: tools.map(mapToolResponse),
        loading: false,
      });
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  // Call a tool
  callTool: async (serverId, toolName, args) => {
    try {
      const result = await invoke<McpToolCallResponse>('call_mcp_tool', {
        request: {
          server_id: serverId,
          tool_name: toolName,
          arguments: args,
        },
      });
      return {
        success: result.success,
        data: result.data,
        error: result.error,
      };
    } catch (err) {
      return {
        success: false,
        error: formatError(err),
      };
    }
  },

  // Refresh prompts from a server
  refreshPrompts: async (serverId) => {
    try {
      set({ loading: true, error: null });
      const prompts = await invoke<McpPromptResponse[]>('list_mcp_prompts', { serverId });
      set({
        prompts: prompts.map(mapPromptResponse),
        loading: false,
      });
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  // Get a prompt
  getPrompt: async (serverId, promptName, args) => {
    try {
      const result = await invoke<string>('get_mcp_prompt', {
        serverId,
        promptName,
        arguments: args,
      });
      return result;
    } catch (err) {
      throw Object.assign(new Error(formatError(err)), { cause: err });
    }
  },
}));
