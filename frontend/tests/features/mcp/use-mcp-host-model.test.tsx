import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getMcpHostStatus: vi.fn(),
  setMcpHostSettings: vi.fn(),
  listMcpHostTools: vi.fn(),
  callMcpHostTool: vi.fn(),
}));

vi.mock('@/lib/api/mcp-host', () => ({
  getMcpHostStatus: mocks.getMcpHostStatus,
  setMcpHostSettings: mocks.setMcpHostSettings,
  listMcpHostTools: mocks.listMcpHostTools,
  callMcpHostTool: mocks.callMcpHostTool,
}));

import { useMcpHostModel } from '@/features/mcp/use-mcp-host-model';
import type { McpHostSettings, McpHostStatus, McpHostTool } from '@/types/models';

const statusKey = ['mcp-host', 'status'] as const;

const initialStatus: McpHostStatus = {
  settings: { enabled: true, disabledTools: [] },
  running: true,
  endpoint: 'http://127.0.0.1:3001/mcp',
};

const updatedStatus: McpHostStatus = {
  settings: { enabled: false, disabledTools: ['forensics.search'] },
  running: false,
  endpoint: 'http://127.0.0.1:3001/mcp',
};

const tools: McpHostTool[] = [
  {
    name: 'forensics.search',
    description: 'Search indexed evidence',
    inputSchema: { type: 'object' },
    outputSchema: { type: 'object' },
    readOnly: true,
    destructive: false,
  },
  {
    name: 'forensics.extract_file',
    description: 'Extract a file into the export directory',
    inputSchema: { type: 'object' },
    outputSchema: { type: 'object' },
    readOnly: false,
    destructive: false,
  },
];

const nextSettings: McpHostSettings = { enabled: false, disabledTools: ['forensics.search'] };

function createQueryClient() {
  return new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
}

function createWrapper(queryClient = createQueryClient()) {
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('useMcpHostModel', () => {
  beforeEach(() => {
    mocks.getMcpHostStatus.mockReset();
    mocks.setMcpHostSettings.mockReset();
    mocks.listMcpHostTools.mockReset();
    mocks.callMcpHostTool.mockReset();
    mocks.getMcpHostStatus.mockResolvedValue(initialStatus);
    mocks.listMcpHostTools.mockResolvedValue(tools);
  });

  it('loads the host status and tool catalogue', async () => {
    const { result } = renderHook(() => useMcpHostModel(), { wrapper: createWrapper() });

    expect(result.current.loading).toBe(true);
    expect(result.current.status).toBeUndefined();
    expect(result.current.tools).toEqual([]);

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.status).toEqual(initialStatus);
    expect(result.current.tools).toEqual(tools);
    expect(result.current.error).toBeNull();
    expect(mocks.getMcpHostStatus).toHaveBeenCalledTimes(1);
    expect(mocks.listMcpHostTools).toHaveBeenCalledTimes(1);
  });

  it('exposes an empty tool list when the host has no tools', async () => {
    mocks.listMcpHostTools.mockResolvedValue([]);

    const { result } = renderHook(() => useMcpHostModel(), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.loading).toBe(false));
    expect(result.current.tools).toEqual([]);
  });

  it('exposes status failures without retrying', async () => {
    mocks.getMcpHostStatus.mockRejectedValue(new Error('host offline'));

    const { result } = renderHook(() => useMcpHostModel(), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.error).toBeInstanceOf(Error));
    expect((result.current.error as Error).message).toBe('host offline');
    expect(result.current.loading).toBe(false);
    expect(mocks.getMcpHostStatus).toHaveBeenCalledTimes(1);
  });

  it('exposes tool catalogue failures', async () => {
    mocks.listMcpHostTools.mockRejectedValue(new Error('tools unavailable'));

    const { result } = renderHook(() => useMcpHostModel(), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.error).toBeInstanceOf(Error));
    expect((result.current.error as Error).message).toBe('tools unavailable');
  });

  it('caches saved settings and invalidates the status query', async () => {
    const queryClient = createQueryClient();
    const setQueryData = vi.spyOn(queryClient, 'setQueryData');
    const invalidateQueries = vi.spyOn(queryClient, 'invalidateQueries');
    mocks.setMcpHostSettings.mockResolvedValue(updatedStatus);

    const { result } = renderHook(() => useMcpHostModel(), {
      wrapper: createWrapper(queryClient),
    });
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await result.current.save(nextSettings);
    });

    expect(mocks.setMcpHostSettings).toHaveBeenCalledTimes(1);
    expect(mocks.setMcpHostSettings.mock.calls[0][0]).toEqual(nextSettings);
    await waitFor(() =>
      expect(setQueryData).toHaveBeenCalledWith(statusKey, updatedStatus),
    );
    await waitFor(() =>
      expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: statusKey }),
    );
  });

  it('reports mutation failures without rejecting save', async () => {
    mocks.setMcpHostSettings.mockRejectedValue(new Error('policy rejected'));
    const { result } = renderHook(() => useMcpHostModel(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    await act(async () => {
      await expect(result.current.save(nextSettings)).resolves.toBeUndefined();
    });

    await waitFor(() => expect(result.current.error).toBeInstanceOf(Error));
    expect((result.current.error as Error).message).toBe('policy rejected');
  });

  it('prefers the mutation error over query errors', async () => {
    mocks.getMcpHostStatus.mockRejectedValue(new Error('status down'));
    mocks.listMcpHostTools.mockRejectedValue(new Error('tools down'));
    mocks.setMcpHostSettings.mockRejectedValue(new Error('save down'));

    const { result } = renderHook(() => useMcpHostModel(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.error).toBeInstanceOf(Error));

    await act(async () => {
      await result.current.save(nextSettings);
    });

    await waitFor(() => expect((result.current.error as Error).message).toBe('save down'));
  });

  it('marks loading while a settings mutation is pending', async () => {
    let resolveSave: (value: McpHostStatus) => void = () => undefined;
    mocks.setMcpHostSettings.mockReturnValue(
      new Promise<McpHostStatus>((resolve) => {
        resolveSave = resolve;
      }),
    );

    const { result } = renderHook(() => useMcpHostModel(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    let pending: Promise<void> = Promise.resolve();
    act(() => {
      pending = result.current.save(nextSettings);
    });
    await waitFor(() => expect(result.current.loading).toBe(true));

    await act(async () => {
      resolveSave(updatedStatus);
      await pending;
    });
    await waitFor(() => expect(result.current.loading).toBe(false));
  });

  it('refetches status and tools on refresh', async () => {
    const { result } = renderHook(() => useMcpHostModel(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    act(() => {
      result.current.refresh();
    });

    await waitFor(() => {
      expect(mocks.getMcpHostStatus).toHaveBeenCalledTimes(2);
      expect(mocks.listMcpHostTools).toHaveBeenCalledTimes(2);
    });
  });

  it('delegates tool calls to the API and forwards results', async () => {
    mocks.callMcpHostTool.mockResolvedValue({ success: true, data: { jobs: 0 } });

    const { result } = renderHook(() => useMcpHostModel(), { wrapper: createWrapper() });

    expect(result.current.callTool).toBe(mocks.callMcpHostTool);
    await expect(result.current.callTool('forensics.list_jobs', {})).resolves.toEqual({
      success: true,
      data: { jobs: 0 },
    });
    expect(mocks.callMcpHostTool).toHaveBeenCalledWith('forensics.list_jobs', {});
  });
});
