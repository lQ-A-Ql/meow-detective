import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getCurrentCase: vi.fn(),
  getDataSources: vi.fn(),
  deleteCase: vi.fn(),
  deleteDataSource: vi.fn(),
  renameDataSource: vi.fn(),
}));

vi.mock('@/lib/api/case', () => ({
  getCurrentCase: mocks.getCurrentCase,
  deleteCase: mocks.deleteCase,
  createCase: vi.fn(),
  openCase: vi.fn(),
  closeCase: vi.fn(),
  renameDataSource: mocks.renameDataSource,
  deleteDataSource: mocks.deleteDataSource,
  removeCaseFromList: vi.fn(),
  getCaseMetrics: vi.fn(),
  getRecentCases: vi.fn(),
  getRecentObjects: vi.fn(),
  getDataSources: mocks.getDataSources,
}));

vi.mock('sonner', () => ({
  toast: { error: vi.fn(), success: vi.fn() },
}));

import {
  useCurrentCase,
  useDataSources,
  useDeleteCase,
  useDeleteDataSource,
  useRenameDataSource,
} from './hooks';

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

describe('case hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useCurrentCase', () => {
    it('fetches current case with the correct query key', async () => {
      const fakeCase = { id: 'case-1', name: 'Test Case' };
      mocks.getCurrentCase.mockResolvedValue(fakeCase);

      const { result } = renderHook(() => useCurrentCase(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data).toEqual(fakeCase);
      expect(mocks.getCurrentCase).toHaveBeenCalledTimes(1);
    });

    it('returns null when no case is open', async () => {
      mocks.getCurrentCase.mockResolvedValue(null);

      const { result } = renderHook(() => useCurrentCase(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data).toBeNull();
    });

    it('does not retry on failure', async () => {
      mocks.getCurrentCase.mockRejectedValue(new Error('no case'));

      const { result } = renderHook(() => useCurrentCase(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isError).toBe(true));
      expect(mocks.getCurrentCase).toHaveBeenCalledTimes(1);
    });
  });

  describe('useDeleteCase', () => {
    it('calls deleteCase API and invalidates all queries on success', async () => {
      mocks.deleteCase.mockResolvedValue('ok');

      const qc = new QueryClient({
        defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
      });
      const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');

      const wrapper = function Wrapper({ children }: { children: React.ReactNode }) {
        return createElement(QueryClientProvider, { client: qc }, children);
      };

      const { result } = renderHook(() => useDeleteCase(), { wrapper });

      await result.current.mutateAsync('/cases/test-case');

      expect(mocks.deleteCase).toHaveBeenCalledWith('/cases/test-case');
      // useDeleteCase calls qc.invalidateQueries() with no args (invalidates all)
      expect(invalidateSpy).toHaveBeenCalledWith();
    });

    it('exposes error state when delete fails', async () => {
      mocks.deleteCase.mockRejectedValue(new Error('permission denied'));

      const { result } = renderHook(() => useDeleteCase(), {
        wrapper: createWrapper(),
      });

      result.current.mutate('/cases/locked-case');

      await waitFor(() => expect(result.current.isError).toBe(true));
      expect(result.current.error).toBeInstanceOf(Error);
      expect((result.current.error as Error).message).toBe('permission denied');
    });
  });

  it('scopes data-source cache entries to the active case id', async () => {
    mocks.getCurrentCase.mockResolvedValue({ id: 'case-1', name: 'Case 1' });
    mocks.getDataSources.mockResolvedValue([]);
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const wrapper = function Wrapper({ children }: { children: React.ReactNode }) {
      return createElement(QueryClientProvider, { client: queryClient }, children);
    };

    const { result } = renderHook(() => useDataSources(), { wrapper });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));

    expect(queryClient.getQueryState(['case', 'data-sources', 'case-1'])).toBeDefined();
  });

  it('invalidates the overview after a data source is renamed', async () => {
    mocks.renameDataSource.mockResolvedValue(undefined);
    const queryClient = createQueryClient();
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
    const { result } = renderHook(() => useRenameDataSource(), {
      wrapper: createWrapper(queryClient),
    });

    await result.current.mutateAsync({ dataSourceId: 'ds-1', name: 'Renamed' });

    expect(invalidateSpy).toHaveBeenCalledWith({
      queryKey: ['analysis', 'case-overview'],
    });
  });

  it('invalidates all analysis projections after a data source is deleted', async () => {
    mocks.deleteDataSource.mockResolvedValue(undefined);
    const queryClient = createQueryClient();
    const invalidateSpy = vi.spyOn(queryClient, 'invalidateQueries');
    const { result } = renderHook(() => useDeleteDataSource(), {
      wrapper: createWrapper(queryClient),
    });

    await result.current.mutateAsync('ds-1');

    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['analysis'] });
  });
});
