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
  type McpPermissionProfile,
  type McpPrompt,
  type McpResource,
  type McpServer as ApiMcpServer,
  type McpServerStatus,
  type McpTool,
} from '@/lib/api/mcp';

interface McpServer {
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

type NewMcpServerInput = Omit<
  McpServer,
  'id' | 'permissions' | 'connected' | 'hasResources' | 'hasTools' | 'hasPrompts'
> & {
  permissions?: McpPermissionProfile;
};

interface McpToolCallResult {
  success: boolean;
  data?: unknown;
  error?: string;
}

interface McpState {
  servers: McpServer[];
  resources: McpResource[];
  tools: McpTool[];
  prompts: McpPrompt[];
  selectedServerId: string | null;
  loading: boolean;
  error: string | null;
  loadConfig: () => Promise<void>;
  saveConfig: () => Promise<void>;
  addServer: (server: NewMcpServerInput) => Promise<void>;
  removeServer: (id: string) => Promise<void>;
  connectServer: (id: string) => Promise<void>;
  disconnectServer: (id: string) => Promise<void>;
  testConnection: (
    transportType: string,
    url?: string,
    command?: string,
    args?: string[],
    permissions?: McpPermissionProfile,
  ) => Promise<{ success: boolean; error?: string }>;
  selectServer: (id: string | null) => void;
  refreshResources: (serverId: string) => Promise<void>;
  refreshTools: (serverId: string) => Promise<void>;
  callTool: (serverId: string, toolName: string, args: unknown) => Promise<McpToolCallResult>;
  refreshPrompts: (serverId: string) => Promise<void>;
  getPrompt: (
    serverId: string,
    promptName: string,
    args?: Record<string, string>,
  ) => Promise<string>;
}

function defaultPermissions(): McpPermissionProfile {
  return {
    resourceAccess: 'readOnly',
    toolAccess: 'disabled',
    promptAccess: 'readOnly',
    networkPolicy: 'localhostOnly',
    allowedTools: [],
    allowedCommands: [],
  };
}

function hasMessage(err: unknown): err is { message: unknown } {
  return typeof err === 'object' && err !== null && 'message' in err;
}

function formatError(err: unknown): string {
  if (typeof err === 'string') return err;
  if (err instanceof Error) return err.message;
  if (hasMessage(err)) {
    return String(err.message);
  }
  return '未知错误';
}

function mapServerResponse(server: ApiMcpServer): McpServer {
  return {
    id: server.id,
    name: server.name,
    transportType: server.transportType,
    url: server.url,
    command: server.command,
    args: server.args,
    enabled: server.enabled,
    autoConnect: server.autoConnect,
    permissions: server.permissions ?? defaultPermissions(),
    connected: false,
    hasResources: false,
    hasTools: false,
    hasPrompts: false,
  };
}

export const useMcpStore = create<McpState>((set, get) => ({
  servers: [],
  resources: [],
  tools: [],
  prompts: [],
  selectedServerId: null,
  loading: false,
  error: null,

  loadConfig: async () => {
    try {
      set({ loading: true, error: null });
      const config: McpConfig = await getMcpConfig();
      set({
        servers: config.servers.map(mapServerResponse),
        loading: false,
      });
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  saveConfig: async () => {
    try {
      const { servers } = get();
      await saveMcpConfig(
        servers.map((server) => ({
          id: server.id,
          name: server.name,
          transportType: server.transportType,
          url: server.url,
          command: server.command,
          args: server.args,
          enabled: server.enabled,
          autoConnect: server.autoConnect,
          permissions: server.permissions,
        })),
      );
    } catch (err) {
      set({ error: formatError(err) });
    }
  },

  addServer: async (server) => {
    try {
      set({ loading: true, error: null });
      await addMcpServer({
        id: crypto.randomUUID(),
        name: server.name,
        transportType: server.transportType,
        url: server.url,
        command: server.command,
        args: server.args,
        enabled: server.enabled,
        autoConnect: server.autoConnect,
        permissions: server.permissions ?? defaultPermissions(),
      });
      await get().loadConfig();
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  removeServer: async (id) => {
    try {
      set({ loading: true, error: null });
      await removeMcpServer(id);
      set((state) => ({
        servers: state.servers.filter((server) => server.id !== id),
        selectedServerId: state.selectedServerId === id ? null : state.selectedServerId,
        loading: false,
      }));
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  connectServer: async (id) => {
    try {
      set({ loading: true, error: null });
      const status: McpServerStatus = await connectMcpServer(id);
      set((state) => ({
        servers: state.servers.map((server) =>
          server.id === id
            ? {
                ...server,
                connected: status.connected,
                hasResources: status.hasResources,
                hasTools: status.hasTools,
                hasPrompts: status.hasPrompts,
                lastError: status.lastError,
              }
            : server,
        ),
        loading: false,
      }));
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  disconnectServer: async (id) => {
    try {
      set({ loading: true, error: null });
      await disconnectMcpServer(id);
      set((state) => ({
        servers: state.servers.map((server) =>
          server.id === id ? { ...server, connected: false } : server,
        ),
        loading: false,
      }));
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  testConnection: async (transportType, url, command, args, permissions) => {
    try {
      const result = await testMcpConnection(
        transportType,
        url,
        command,
        args,
        permissions ?? defaultPermissions(),
      );
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

  selectServer: (id) => {
    set((state) => {
      if (id === state.selectedServerId) {
        return state;
      }
      return {
        selectedServerId: id,
        resources: [],
        tools: [],
        prompts: [],
      };
    });
  },

  refreshResources: async (serverId) => {
    try {
      set({ loading: true, error: null });
      const resources = await listMcpResources(serverId);
      set({ resources, loading: false });
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  refreshTools: async (serverId) => {
    try {
      set({ loading: true, error: null });
      const tools = await listMcpTools(serverId);
      set({ tools, loading: false });
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

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

  refreshPrompts: async (serverId) => {
    try {
      set({ loading: true, error: null });
      const prompts = await listMcpPrompts(serverId);
      set({ prompts, loading: false });
    } catch (err) {
      set({ error: formatError(err), loading: false });
    }
  },

  getPrompt: async (serverId, promptName, args) => {
    try {
      return await getMcpPrompt(serverId, promptName, args);
    } catch (err) {
      throw Object.assign(new Error(formatError(err)), { cause: err });
    }
  },
}));
