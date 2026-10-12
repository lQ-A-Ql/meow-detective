import { render, screen, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { McpResourceListContainer } from '@/features/mcp/containers/McpResourceListContainer';
import { McpToolListContainer } from '@/features/mcp/containers/McpToolListContainer';
import { useMcpStore } from '@/stores/mcp-store';
import { listMcpResources, listMcpTools, type McpResource, type McpTool } from '@/lib/api/mcp';

vi.mock('@/lib/api/mcp', async (importOriginal) => ({
  ...await importOriginal<typeof import('@/lib/api/mcp')>(),
  listMcpResources: vi.fn(), listMcpTools: vi.fn(),
}));

vi.mock('@/features/mcp/components/McpResourceList', () => ({
  McpResourceList: ({ resources, loading }: { resources: McpResource[]; loading: boolean }) =>
    <div data-testid="resources">{loading ? 'loading' : resources.map((item) => item.name).join(',')}</div>,
}));
vi.mock('@/features/mcp/components/McpToolList', () => ({
  McpToolList: ({ tools, loading }: { tools: McpTool[]; loading: boolean }) =>
    <div data-testid="tools">{loading ? 'loading' : tools.map((item) => item.name).join(',')}</div>,
}));

describe('MCP data panes', () => {
  beforeEach(() => {
    useMcpStore.setState({ servers: [], resourcesByServer: {}, toolsByServer: {},
      resourceLoadingByServer: {}, toolLoadingByServer: {}, resourceErrorsByServer: {}, toolErrorsByServer: {} });
  });

  it('renders uncached panes without a subscription loop, then displays the loaded data', async () => {
    let resolveResources!: (items: McpResource[]) => void;
    let resolveTools!: (items: McpTool[]) => void;
    vi.mocked(listMcpResources).mockImplementation(() => new Promise((resolve) => { resolveResources = resolve; }));
    vi.mocked(listMcpTools).mockImplementation(() => new Promise((resolve) => { resolveTools = resolve; }));

    render(<><McpResourceListContainer serverId="mock-local" /><McpToolListContainer serverId="mock-local" /></>);
    expect(screen.getByTestId('resources')).toHaveTextContent('loading');
    expect(screen.getByTestId('tools')).toHaveTextContent('loading');
    expect(listMcpResources).toHaveBeenCalledTimes(1);
    expect(listMcpTools).toHaveBeenCalledTimes(1);

    resolveResources([{ uri: 'forensics://mock/summary', name: 'Mock summary' }]);
    resolveTools([{ name: 'mockSearch', description: '', inputSchema: { type: 'object' } }]);
    await waitFor(() => expect(screen.getByTestId('resources')).toHaveTextContent('Mock summary'));
    await waitFor(() => expect(screen.getByTestId('tools')).toHaveTextContent('mockSearch'));
  });
});
