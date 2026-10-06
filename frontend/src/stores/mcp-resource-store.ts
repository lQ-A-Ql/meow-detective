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
  resourcesByServer: Record<string, McpResource[]>;
  toolsByServer: Record<string, McpTool[]>;
  promptsByServer: Record<string, McpPrompt[]>;
  resourceLoadingByServer: Record<string, boolean>;
  toolLoadingByServer: Record<string, boolean>;
  promptLoadingByServer: Record<string, boolean>;
  resourceErrorsByServer: Record<string, string | null>;
  toolErrorsByServer: Record<string, string | null>;
  promptErrorsByServer: Record<string, string | null>;
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
  resourcesByServer: {},
  toolsByServer: {},
  promptsByServer: {},
  resourceLoadingByServer: {},
  toolLoadingByServer: {},
  promptLoadingByServer: {},
  resourceErrorsByServer: {},
  toolErrorsByServer: {},
  promptErrorsByServer: {},

  refreshResources: async (serverId) => {
    set((state) => ({
      resourceLoadingByServer: { ...state.resourceLoadingByServer, [serverId]: true },
      resourceErrorsByServer: { ...state.resourceErrorsByServer, [serverId]: null },
    }));
    try {
      const resources = await listMcpResources(serverId);
      set((state) => ({
        resources,
        resourcesByServer: { ...state.resourcesByServer, [serverId]: resources },
        resourceLoadingByServer: { ...state.resourceLoadingByServer, [serverId]: false },
      }));
    } catch (err) {
      const error = formatError(err);
      set((state) => ({
        resourceLoadingByServer: { ...state.resourceLoadingByServer, [serverId]: false },
        resourceErrorsByServer: { ...state.resourceErrorsByServer, [serverId]: error },
      }));
    }
  },

  refreshTools: async (serverId) => {
    set((state) => ({
      toolLoadingByServer: { ...state.toolLoadingByServer, [serverId]: true },
      toolErrorsByServer: { ...state.toolErrorsByServer, [serverId]: null },
    }));
    try {
      const tools = await listMcpTools(serverId);
      set((state) => ({
        tools,
        toolsByServer: { ...state.toolsByServer, [serverId]: tools },
        toolLoadingByServer: { ...state.toolLoadingByServer, [serverId]: false },
      }));
    } catch (err) {
      const error = formatError(err);
      set((state) => ({
        toolLoadingByServer: { ...state.toolLoadingByServer, [serverId]: false },
        toolErrorsByServer: { ...state.toolErrorsByServer, [serverId]: error },
      }));
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
    set((state) => ({
      promptLoadingByServer: { ...state.promptLoadingByServer, [serverId]: true },
      promptErrorsByServer: { ...state.promptErrorsByServer, [serverId]: null },
    }));
    try {
      const prompts = await listMcpPrompts(serverId);
      set((state) => ({
        prompts,
        promptsByServer: { ...state.promptsByServer, [serverId]: prompts },
        promptLoadingByServer: { ...state.promptLoadingByServer, [serverId]: false },
      }));
    } catch (err) {
      const error = formatError(err);
      set((state) => ({
        promptLoadingByServer: { ...state.promptLoadingByServer, [serverId]: false },
        promptErrorsByServer: { ...state.promptErrorsByServer, [serverId]: error },
      }));
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
    set({
      resources: [],
      tools: [],
      prompts: [],
      resourcesByServer: {},
      toolsByServer: {},
      promptsByServer: {},
      resourceLoadingByServer: {},
      toolLoadingByServer: {},
      promptLoadingByServer: {},
      resourceErrorsByServer: {},
      toolErrorsByServer: {},
      promptErrorsByServer: {},
    });
  },
});
