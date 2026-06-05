import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  callMcpTool,
  connectMcpServer,
  getMcpConfig,
  getMcpPrompt,
  testMcpConnection,
} from '@/lib/api/mcp';
import { useMcpStore } from './mcp-store';

vi.mock('@/lib/api/mcp', () => ({
  addMcpServer: vi.fn(),
  callMcpTool: vi.fn(),
  connectMcpServer: vi.fn(),
  disconnectMcpServer: vi.fn(),
  getMcpConfig: vi.fn(),
  getMcpPrompt: vi.fn(),
  listMcpPrompts: vi.fn(),
  listMcpResources: vi.fn(),
  listMcpTools: vi.fn(),
  removeMcpServer: vi.fn(),
  saveMcpConfig: vi.fn(),
  testMcpConnection: vi.fn(),
}));

const getMcpConfigMock = vi.mocked(getMcpConfig);
const connectMcpServerMock = vi.mocked(connectMcpServer);
const callMcpToolMock = vi.mocked(callMcpTool);
const getMcpPromptMock = vi.mocked(getMcpPrompt);
const testMcpConnectionMock = vi.mocked(testMcpConnection);

function resetMcpStore() {
  useMcpStore.setState({
    servers: [],
    resources: [],
    tools: [],
    prompts: [],
    selectedServerId: null,
    loading: false,
    error: null,
  });
}

describe('mcp-store contract baseline', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetMcpStore();
  });

  it('loads API-normalized MCP config responses into store state', async () => {
    getMcpConfigMock.mockResolvedValueOnce({
      servers: [
        {
          id: 'srv-1',
          name: 'Local MCP',
          transportType: 'stdio',
          command: 'node',
          args: ['server.js'],
          enabled: true,
          autoConnect: true,
        },
      ],
      resources: {},
      tools: {},
    });

    await useMcpStore.getState().loadConfig();

    expect(getMcpConfigMock).toHaveBeenCalledWith();
    expect(useMcpStore.getState().servers[0]).toMatchObject({
      id: 'srv-1',
      transportType: 'stdio',
      autoConnect: true,
      connected: false,
      hasResources: false,
      hasTools: false,
      hasPrompts: false,
    });
  });

  it('connectServer delegates to the MCP API layer and consumes normalized status', async () => {
    useMcpStore.setState({
      servers: [
        {
          id: 'srv-1',
          name: 'Local MCP',
          transportType: 'sse',
          enabled: true,
          autoConnect: false,
          connected: false,
          hasResources: false,
          hasTools: false,
          hasPrompts: false,
        },
      ],
    });
    connectMcpServerMock.mockResolvedValueOnce({
      id: 'srv-1',
      name: 'Local MCP',
      connected: true,
      hasResources: true,
      hasTools: true,
      hasPrompts: false,
      lastError: 'none',
    });

    await useMcpStore.getState().connectServer('srv-1');

    expect(connectMcpServerMock).toHaveBeenCalledWith('srv-1');
    expect(useMcpStore.getState().servers[0]).toMatchObject({
      connected: true,
      hasResources: true,
      hasTools: true,
      hasPrompts: false,
      lastError: 'none',
    });
  });

  it('callTool delegates serverId and toolName to the MCP API layer', async () => {
    callMcpToolMock.mockResolvedValueOnce({ success: true, data: { rows: 3 } });

    const result = await useMcpStore.getState().callTool('srv-1', 'queryTimeline', { limit: 3 });

    expect(callMcpToolMock).toHaveBeenCalledWith('srv-1', 'queryTimeline', { limit: 3 });
    expect(result).toEqual({ success: true, data: { rows: 3 }, error: undefined });
  });

  it('getPrompt delegates promptName to the MCP API layer', async () => {
    getMcpPromptMock.mockResolvedValueOnce('prompt body');

    const result = await useMcpStore.getState().getPrompt('srv-1', 'summarize', { file: 'mft.csv' });

    expect(getMcpPromptMock).toHaveBeenCalledWith('srv-1', 'summarize', { file: 'mft.csv' });
    expect(result).toBe('prompt body');
  });

  it('testConnection delegates transport details to the MCP API layer', async () => {
    testMcpConnectionMock.mockResolvedValueOnce({ success: true });

    const result = await useMcpStore.getState().testConnection('stdio', undefined, 'node', ['server.js']);

    expect(testMcpConnectionMock).toHaveBeenCalledWith('stdio', undefined, 'node', ['server.js']);
    expect(result).toEqual({ success: true, error: undefined });
  });

  it('does not crash when API normalization returns empty config for malformed backend data', async () => {
    getMcpConfigMock.mockResolvedValueOnce({ servers: [], resources: {}, tools: {} });

    await useMcpStore.getState().loadConfig();

    expect(useMcpStore.getState()).toMatchObject({
      servers: [],
      loading: false,
      error: null,
    });
  });

  it('does not crash when API normalization returns fallback resource, tool, and prompt data', async () => {
    const { listMcpPrompts, listMcpResources, listMcpTools } = await import('@/lib/api/mcp');

    vi.mocked(listMcpResources).mockResolvedValueOnce([{ uri: '', name: '', mimeType: undefined }]);
    vi.mocked(listMcpTools).mockResolvedValueOnce([{ name: '', description: '', inputSchema: { type: 'object' } }]);
    vi.mocked(listMcpPrompts).mockResolvedValueOnce([{ name: '', arguments: [] }]);

    await useMcpStore.getState().refreshResources('srv-1');
    await useMcpStore.getState().refreshTools('srv-1');
    await useMcpStore.getState().refreshPrompts('srv-1');

    expect(useMcpStore.getState()).toMatchObject({
      resources: [{ uri: '', name: '', mimeType: undefined }],
      tools: [{ name: '', description: '', inputSchema: { type: 'object' } }],
      prompts: [{ name: '', arguments: [] }],
      loading: false,
      error: null,
    });
  });
});
