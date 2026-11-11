import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  useCurrentCase: vi.fn(),
  callMcpHostTool: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.useCurrentCase,
}));

vi.mock('@/lib/api/mcp-host', () => ({
  callMcpHostTool: mocks.callMcpHostTool,
}));

import { useMcpActivityModel } from '@/features/mcp/use-mcp-activity-model';
import type { JobSnapshot } from '@/types/jobs';
import type { McpHostMetrics } from '@/types/mcpHost';

const metrics: McpHostMetrics = {
  running: true,
  activeRequests: 1,
  queuedRequests: 0,
  taskOwnerCount: 1,
  totalCalls: 12,
  successfulCalls: 11,
  failedCalls: 1,
  cancelledCalls: 0,
  totalDurationMs: 2400,
  averageDurationMs: 200,
  currentRssMb: 512,
  peakRssMb: 640,
};

const jobs: JobSnapshot[] = [
  {
    id: 'job-1',
    name: 'Import disk01',
    scope: 'import',
    progress: 42,
    status: 'running',
    detail: 'Extracting filesystem',
    warningCount: 0,
    skippedCount: 0,
    failedCount: 0,
    partial: false,
  },
];

const metricsTool = 'forensics.get_mcp_metrics';
const jobsTool = 'forensics.list_jobs';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

function setCurrentCase(value: unknown) {
  mocks.useCurrentCase.mockReturnValue(value as never);
}

function callsFor(tool: string) {
  return mocks.callMcpHostTool.mock.calls.filter(([name]) => name === tool).length;
}

describe('useMcpActivityModel', () => {
  beforeEach(() => {
    mocks.useCurrentCase.mockReset();
    mocks.callMcpHostTool.mockReset();
    setCurrentCase({ data: { id: 'case-1' }, isSuccess: true });
    mocks.callMcpHostTool.mockImplementation(async (name: string) => {
      if (name === metricsTool) return { success: true, data: metrics };
      if (name === jobsTool) return { success: true, data: jobs };
      return { success: false, error: 'unknown tool' };
    });
  });

  it('stays unavailable while the host is disabled', () => {
    const { result } = renderHook(() => useMcpActivityModel(false), { wrapper: createWrapper() });

    expect(result.current.available).toBe(false);
    expect(result.current.metrics).toBeUndefined();
    expect(result.current.jobs).toBeUndefined();
    expect(result.current.error).toBe(false);
    expect(result.current.loading).toBe(false);
    expect(mocks.callMcpHostTool).not.toHaveBeenCalled();
  });

  it('stays unavailable until the current case has loaded', () => {
    setCurrentCase({ data: undefined, isSuccess: false });

    const { result } = renderHook(() => useMcpActivityModel(true), { wrapper: createWrapper() });

    expect(result.current.available).toBe(false);
    expect(mocks.callMcpHostTool).not.toHaveBeenCalled();
  });

  it('stays unavailable when the loaded case has no id', () => {
    setCurrentCase({ data: {}, isSuccess: true });

    const { result } = renderHook(() => useMcpActivityModel(true), { wrapper: createWrapper() });

    expect(result.current.available).toBe(false);
    expect(mocks.callMcpHostTool).not.toHaveBeenCalled();
  });

  it('loads metrics and jobs through the MCP tool channel', async () => {
    const { result } = renderHook(() => useMcpActivityModel(true), { wrapper: createWrapper() });

    expect(result.current.available).toBe(true);
    expect(result.current.loading).toBe(true);

    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.metrics).toEqual(metrics);
    expect(result.current.jobs).toEqual(jobs);
    expect(result.current.error).toBe(false);
    expect(mocks.callMcpHostTool).toHaveBeenCalledWith(metricsTool, {});
    expect(mocks.callMcpHostTool).toHaveBeenCalledWith(jobsTool, {});
  });

  it('reports an error when a tool result is unsuccessful', async () => {
    mocks.callMcpHostTool.mockImplementation(async (name: string) => {
      if (name === metricsTool) return { success: false, error: 'metrics disabled' };
      return { success: true, data: jobs };
    });

    const { result } = renderHook(() => useMcpActivityModel(true), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.error).toBe(true));
    await waitFor(() => expect(result.current.loading).toBe(false));

    expect(result.current.metrics).toBeUndefined();
    expect(result.current.jobs).toEqual(jobs);
    expect(callsFor(metricsTool)).toBe(1);
  });

  it('reports an error when a successful result carries no data', async () => {
    mocks.callMcpHostTool.mockImplementation(async (name: string) => {
      if (name === metricsTool) return { success: true };
      return { success: true, data: jobs };
    });

    const { result } = renderHook(() => useMcpActivityModel(true), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.error).toBe(true));
    expect(result.current.metrics).toBeUndefined();
  });

  it('hides cached activity when the host becomes disabled', async () => {
    const { result, rerender } = renderHook(
      ({ enabled }: { enabled: boolean }) => useMcpActivityModel(enabled),
      { wrapper: createWrapper(), initialProps: { enabled: true } },
    );
    await waitFor(() => expect(result.current.jobs).toEqual(jobs));

    rerender({ enabled: false });

    expect(result.current.available).toBe(false);
    expect(result.current.metrics).toBeUndefined();
    expect(result.current.jobs).toBeUndefined();
  });

  it('refetches both activity queries on refresh', async () => {
    const { result } = renderHook(() => useMcpActivityModel(true), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.loading).toBe(false));

    act(() => {
      result.current.refresh();
    });

    await waitFor(() => {
      expect(callsFor(metricsTool)).toBe(2);
      expect(callsFor(jobsTool)).toBe(2);
    });
  });

  it('does not refresh while activity is unavailable', () => {
    const { result } = renderHook(() => useMcpActivityModel(false), { wrapper: createWrapper() });

    act(() => {
      result.current.refresh();
    });

    expect(mocks.callMcpHostTool).not.toHaveBeenCalled();
  });
});
