import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getReportHistory: vi.fn(),
  getReportTemplates: vi.fn(),
}));

vi.mock('@/lib/api/reports', () => ({
  getReportHistory: mocks.getReportHistory,
  getReportTemplates: mocks.getReportTemplates,
}));

import { useReportHistory, useReportTemplates } from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('reports hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getReportTemplates.mockResolvedValue([
      { id: 'tpl-1', name: 'Summary Report', format: 'html' },
    ]);
    mocks.getReportHistory.mockResolvedValue([
      { id: 'rpt-1', templateId: 'tpl-1', createdAt: '2026-06-01T10:00:00Z' },
    ]);
  });

  it('fetches report templates', async () => {
    const { result } = renderHook(() => useReportTemplates(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getReportTemplates).toHaveBeenCalledTimes(1);
    expect(result.current.data).toBeDefined();
    expect(result.current.data!).toHaveLength(1);
  });

  it('returns templates with expected shape', async () => {
    const { result } = renderHook(() => useReportTemplates(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toBeDefined();
    expect(result.current.data![0]).toEqual(
      expect.objectContaining({ id: 'tpl-1', name: 'Summary Report' }),
    );
  });

  it('fetches report history', async () => {
    const { result } = renderHook(() => useReportHistory(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getReportHistory).toHaveBeenCalledTimes(1);
    expect(result.current.data).toHaveLength(1);
  });

  it('returns empty arrays when no data exists', async () => {
    mocks.getReportTemplates.mockResolvedValue([]);
    mocks.getReportHistory.mockResolvedValue([]);

    const wrapper = createWrapper();
    const templates = renderHook(() => useReportTemplates(), { wrapper });
    const history = renderHook(() => useReportHistory(), { wrapper });

    await waitFor(() => expect(templates.result.current.isSuccess).toBe(true));
    await waitFor(() => expect(history.result.current.isSuccess).toBe(true));

    expect(templates.result.current.data).toEqual([]);
    expect(history.result.current.data).toEqual([]);
  });

  it('does not retry on failure', async () => {
    mocks.getReportTemplates.mockRejectedValue(new Error('server error'));

    const { result } = renderHook(() => useReportTemplates(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(mocks.getReportTemplates).toHaveBeenCalledTimes(1);
  });
});
