import { create } from 'zustand';
import { createMcpResourceSlice, type McpResourceSlice } from './mcp-resource-store';
import { createMcpServerSlice, type McpServerSlice } from './mcp-server-store';

type McpState = McpServerSlice & McpResourceSlice;

export const useMcpStore = create<McpState>()((...args) => ({
  ...createMcpServerSlice(...args),
  ...createMcpResourceSlice(...args),
}));

export type { McpServer, McpToolCallResult, NewMcpServerInput } from './mcp-types';
