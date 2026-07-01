import type { StateCreator } from 'zustand';
import {
  addMcpServer,
  connectMcpServer,
  disconnectMcpServer,
  getMcpConfig,
  removeMcpServer,
  saveMcpConfig,
  testMcpConnection,
  type McpConfig,
  type McpPermissionProfile,
  type McpServer as ApiMcpServer,
  type McpServerStatus,
} from '@/lib/api/mcp';
import { defaultPermissions, formatError } from './mcp-error-utils';
import type { McpResourceSlice } from './mcp-resource-store';
import type { McpServer, NewMcpServerInput } from './mcp-types';

export interface McpServerSlice {
  servers: McpServer[];
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

export const createMcpServerSlice: StateCreator<
  McpServerSlice & Pick<McpResourceSlice, 'clearResources'>,
  [],
  [],
  McpServerSlice
> = (set, get) => ({
  servers: [],
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
    if (id === get().selectedServerId) {
      return;
    }
    set({ selectedServerId: id });
    get().clearResources();
  },
});
