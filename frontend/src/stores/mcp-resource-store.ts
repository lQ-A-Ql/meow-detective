import type { StateCreator } from 'zustand';
import {
  callMcpTool,
  getMcpPrompt,
  listMcpPrompts,
  listMcpResources,
  listMcpTools,
  type McpPrompt,
  type McpResource,
  type McpTool,
} from '@/lib/api/mcp';
import { formatError } from './mcp-error-utils';
import type { McpToolCallResult } from './mcp-types';

export interface McpResourceSlice {
  resources: McpResource[];
  tools: McpTool[];
  prompts: McpPrompt[];
  refreshResources: (serverId: string) => Promise<void>;
  refreshTools: (serverId: string) => Promise<void>;
  callTool: (serverId: string, toolName: string, args: unknown) => Promise<McpToolCallResult>;
  refreshPrompts: (serverId: string) => Promise<void>;
  getPrompt: (
    serverId: string,
    promptName: string,
    args?: Record<string, string>,
  ) => Promise<string>;
  /** Clears cached resources/tools/prompts, e.g. when the selected server changes. */
  clearResources: () => void;
}

export const createMcpResourceSlice: StateCreator<
  McpResourceSlice & { loading: boolean; error: string | null },
  [],
  [],
  McpResourceSlice
> = (set) => ({
  resources: [],
  tools: [],
  prompts: [],

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

  clearResources: () => {
    set({ resources: [], tools: [], prompts: [] });
  },
});
