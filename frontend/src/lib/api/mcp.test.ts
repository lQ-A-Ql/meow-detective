import { beforeEach, describe, expect, it, vi } from 'vitest';
import { apiClient } from './client';
import {
  addMcpServer,
  callMcpTool,
  connectMcpServer,
  getMcpConfig,
  getMcpPrompt,
  listMcpPrompts,
  listMcpResources,
  listMcpTools,
  removeMcpServer,
  saveMcpConfig,
  testMcpConnection,
} from './mcp';

vi.mock('./client', () => ({
  apiClient: {
    request: vi.fn(),
  },
}));

const requestMock = vi.mocked(apiClient.request);

describe('mcp API layer', () => {
  beforeEach(() => {
    requestMock.mockReset();
  });

  it('gets MCP config with the expected command and mock fallback', async () => {
    requestMock.mockResolvedValueOnce({
      servers: [
        {
          id: 'srv-1',
          name: 'Local MCP',
          transport_type: 'stdio',
          command: 'node',
          args: ['server.js'],
          enabled: true,
          auto_connect: true,
        },
      ],
      resources: { 'file:///case': true, ignored: 'yes' },
      tools: { lookup: false, ignored: 1 },
    });

    const result = await getMcpConfig();

    expect(requestMock).toHaveBeenCalledWith('get_mcp_config', expect.any(Function));
    expect(result).toEqual({
      servers: [
        {
          id: 'srv-1',
          name: 'Local MCP',
          transportType: 'stdio',
          url: undefined,
          command: 'node',
          args: ['server.js'],
          enabled: true,
          autoConnect: true,
        },
      ],
      resources: { 'file:///case': true },
      tools: { lookup: false },
    });
  });

  it('saves server config with a camelCase top-level config arg and current nested snake_case DTO fields', async () => {
    requestMock.mockResolvedValueOnce(undefined);

    await saveMcpConfig([
      {
        id: 'srv-1',
        name: 'Local MCP',
        transportType: 'stdio',
        command: 'node',
        args: ['server.js'],
        enabled: true,
        autoConnect: false,
      },
    ]);

    expect(requestMock).toHaveBeenCalledWith('save_mcp_config', expect.any(Function), {
      config: {
        servers: [
          {
            id: 'srv-1',
            name: 'Local MCP',
            transport_type: 'stdio',
            url: undefined,
            command: 'node',
            args: ['server.js'],
            enabled: true,
            auto_connect: false,
          },
        ],
        resources: {},
        tools: {},
      },
    });
  });

  it('adds a server through the expected server arg shape', async () => {
    requestMock.mockResolvedValueOnce({
      id: 'srv-1',
      name: 'Remote MCP',
      connected: false,
      has_resources: false,
      has_tools: false,
      has_prompts: false,
    });

    const result = await addMcpServer({
      id: 'srv-1',
      name: 'Remote MCP',
      transportType: 'sse',
      url: 'http://127.0.0.1:3000/sse',
      enabled: true,
      autoConnect: true,
    });

    expect(requestMock).toHaveBeenCalledWith('add_mcp_server', expect.any(Function), {
      server: {
        id: 'srv-1',
        name: 'Remote MCP',
        transport_type: 'sse',
        url: 'http://127.0.0.1:3000/sse',
        command: undefined,
        args: undefined,
        enabled: true,
        auto_connect: true,
      },
    });
    expect(result).toMatchObject({ id: 'srv-1', hasResources: false, hasTools: false, hasPrompts: false });
  });

  it('uses camelCase top-level serverId args for direct server commands', async () => {
    requestMock.mockResolvedValueOnce({
      id: 'srv-1',
      name: 'Local MCP',
      connected: false,
      has_resources: false,
      has_tools: false,
      has_prompts: false,
    });
    const status = await connectMcpServer('srv-1');
    expect(requestMock).toHaveBeenLastCalledWith('connect_mcp_server', expect.any(Function), { serverId: 'srv-1' });
    expect(status).toMatchObject({ id: 'srv-1', connected: false, hasResources: false });

    requestMock.mockResolvedValueOnce(undefined);
    await removeMcpServer('srv-1');
    expect(requestMock).toHaveBeenLastCalledWith('remove_mcp_server', expect.any(Function), { serverId: 'srv-1' });
  });

  it('keeps nested snake_case request fields for test connection', async () => {
    requestMock.mockResolvedValueOnce({ success: true, error: 404 });

    const result = await testMcpConnection('stdio', undefined, 'node', ['server.js']);

    expect(requestMock).toHaveBeenCalledWith('test_mcp_connection', expect.any(Function), {
      request: {
        transport_type: 'stdio',
        url: undefined,
        command: 'node',
        args: ['server.js'],
      },
    });
    expect(result).toEqual({ success: true, error: undefined });
  });

  it('uses camelCase top-level serverId args for list commands', async () => {
    requestMock.mockResolvedValueOnce([{ uri: 'file:///case', name: 'Case', description: 5, mime_type: 'text/plain' }]);
    const resources = await listMcpResources('srv-1');
    expect(requestMock).toHaveBeenLastCalledWith('list_mcp_resources', expect.any(Function), { serverId: 'srv-1' });
    expect(resources).toEqual([{ uri: 'file:///case', name: 'Case', description: undefined, mimeType: 'text/plain' }]);

    requestMock.mockResolvedValueOnce([{ name: 'lookup', description: 'Lookup evidence', input_schema: { type: 'object' } }]);
    const tools = await listMcpTools('srv-1');
    expect(requestMock).toHaveBeenLastCalledWith('list_mcp_tools', expect.any(Function), { serverId: 'srv-1' });
    expect(tools).toEqual([{ name: 'lookup', description: 'Lookup evidence', inputSchema: { type: 'object' } }]);

    requestMock.mockResolvedValueOnce([{ name: 'summarize', description: 'Summarize', arguments: [{ name: 'file', required: true }, 'bad'] }]);
    const prompts = await listMcpPrompts('srv-1');
    expect(requestMock).toHaveBeenLastCalledWith('list_mcp_prompts', expect.any(Function), { serverId: 'srv-1' });
    expect(prompts).toEqual([{ name: 'summarize', description: 'Summarize', arguments: [{ name: 'file', description: undefined, required: true }] }]);
  });

  it('keeps nested snake_case request fields for tool calls', async () => {
    requestMock.mockResolvedValueOnce({ success: true, data: { rows: 3 } });

    const result = await callMcpTool('srv-1', 'queryTimeline', { limit: 3 });

    expect(requestMock).toHaveBeenCalledWith('call_mcp_tool', expect.any(Function), {
      request: {
        server_id: 'srv-1',
        tool_name: 'queryTimeline',
        arguments: { limit: 3 },
      },
    });
    expect(result).toEqual({ success: true, data: { rows: 3 }, error: undefined });
  });

  it('uses camelCase top-level promptName args for prompt retrieval', async () => {
    requestMock.mockResolvedValueOnce('prompt body');

    await getMcpPrompt('srv-1', 'summarize', { file: 'mft.csv' });

    expect(requestMock).toHaveBeenCalledWith('get_mcp_prompt', expect.any(Function), {
      serverId: 'srv-1',
      promptName: 'summarize',
      arguments: { file: 'mft.csv' },
    });
  });

  it('safely normalizes malformed config, status, resource, tool, prompt, and tool-call responses', async () => {
    requestMock.mockResolvedValueOnce({ servers: undefined, resources: null, tools: { lookup: true } });
    await expect(getMcpConfig()).resolves.toEqual({ servers: [], resources: {}, tools: { lookup: true } });

    requestMock.mockResolvedValueOnce({ connected: 'yes', has_resources: true, has_tools: 1, last_error: { message: 'boom' } });
    await expect(connectMcpServer('srv-1')).resolves.toEqual({
      id: '',
      name: '',
      connected: false,
      hasResources: true,
      hasTools: false,
      hasPrompts: false,
      lastError: undefined,
    });

    requestMock.mockResolvedValueOnce('not a list');
    await expect(listMcpResources('srv-1')).resolves.toEqual([]);

    requestMock.mockResolvedValueOnce([{ name: 7, input_schema: 'bad' }]);
    await expect(listMcpTools('srv-1')).resolves.toEqual([{ name: '', description: '', inputSchema: { type: 'object' } }]);

    requestMock.mockResolvedValueOnce([{ arguments: undefined }]);
    await expect(listMcpPrompts('srv-1')).resolves.toEqual([{ name: '', description: undefined, arguments: [] }]);

    requestMock.mockResolvedValueOnce({ success: 'true', error: 42 });
    await expect(callMcpTool('srv-1', 'lookup', {})).resolves.toEqual({ success: false, data: undefined, error: undefined });
  });
});
