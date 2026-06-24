import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getCurrentCase: vi.fn(),
  deleteCase: vi.fn(),
}));

vi.mock('@/lib/api/case', () => ({
  getCurrentCase: mocks.getCurrentCase,
  deleteCase: mocks.deleteCase,
  createCase: vi.fn(),
  createAnalysisDemoCase: vi.fn(),
  openCase: vi.fn(),
  closeCase: vi.fn(),
  renameDataSource: vi.fn(),
  deleteDataSource: vi.fn(),
  removeCaseFromList: vi.fn(),
  getCaseMetrics: vi.fn(),
  getRecentCases: vi.fn(),
  getRecentObjects: vi.fn(),
  getDataSources: vi.fn(),
}));

vi.mock('sonner', () => ({
  toast: { error: vi.fn(), success: vi.fn() },
}));

import { useCurrentCase, useDeleteCase } from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
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
});
