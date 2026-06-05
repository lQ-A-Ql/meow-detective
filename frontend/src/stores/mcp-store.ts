import { create } from 'zustand';
import {
  addMcpServer,
  callMcpTool,
  connectMcpServer,
  disconnectMcpServer,
  getMcpConfig,
  getMcpPrompt,
  listMcpPrompts,
  listMcpResources,
  listMcpTools,
  removeMcpServer,
  saveMcpConfig,
  testMcpConnection,
  type McpConfig,
  type McpPrompt,
  type McpResource,
  type McpServer as ApiMcpServer,
  type McpServerStatus,
  type McpTool,
} from '@/lib/api/mcp';

// ============================================
// 类型定义
// ============================================

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

/** MCP Tool Call Result */
interface McpToolCallResult {
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
function mapServerResponse(s: ApiMcpServer): McpServer {
  return {
    id: s.id,
    name: s.name,
    transportType: s.transportType,
    url: s.url,
    command: s.command,
    args: s.args,
    enabled: s.enabled,
    autoConnect: s.autoConnect,
    connected: false,
    hasResources: false,
    hasTools: false,
    hasPrompts: false,
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
      const config: McpConfig = await getMcpConfig();

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
      await saveMcpConfig(servers.map((s) => ({
          id: s.id,
          name: s.name,
          transportType: s.transportType,
          url: s.url,
          command: s.command,
          args: s.args,
          enabled: s.enabled,
          autoConnect: s.autoConnect,
        })));
    } catch (err) {
      set({ error: formatError(err) });
    }
  },

  // Add a new server
  addServer: async (server) => {
    try {
      set({ loading: true, error: null });
      const id = crypto.randomUUID();

      await addMcpServer({
        id,
        name: server.name,
        transportType: server.transportType,
        url: server.url,
        command: server.command,
        args: server.args,
        enabled: server.enabled,
        autoConnect: server.autoConnect,
      });
      await get().loadConfig();
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  // Remove a server
  removeServer: async (id) => {
    try {
      set({ loading: true, error: null });
      await removeMcpServer(id);

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
      const status: McpServerStatus = await connectMcpServer(id);

      set((state) => ({
        servers: state.servers.map((s) =>
          s.id === id
            ? {
                ...s,
                connected: status.connected,
                hasResources: status.hasResources,
                hasTools: status.hasTools,
                hasPrompts: status.hasPrompts,
                lastError: status.lastError,
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
      await disconnectMcpServer(id);

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
      const result = await testMcpConnection(transportType, url, command, args);
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
      const resources: McpResource[] = await listMcpResources(serverId);
      set({
        resources,
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
      const tools: McpTool[] = await listMcpTools(serverId);
      set({
        tools,
        loading: false,
      });
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  // Call a tool
  callTool: async (serverId, toolName, args) => {
    try {
      const result = await callMcpTool(serverId, toolName, args);
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
      const prompts: McpPrompt[] = await listMcpPrompts(serverId);
      set({
        prompts,
        loading: false,
      });
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  // Get a prompt
  getPrompt: async (serverId, promptName, args) => {
    try {
      const result = await getMcpPrompt(serverId, promptName, args);
      return result;
    } catch (err) {
      throw Object.assign(new Error(formatError(err)), { cause: err });
    }
  },
}));
