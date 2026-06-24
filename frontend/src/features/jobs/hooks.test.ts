import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getJobsSnapshot: vi.fn(),
  getWarnings: vi.fn(),
  getTraceItems: vi.fn(),
}));

vi.mock('@/lib/api/jobs', () => ({
  getJobsSnapshot: mocks.getJobsSnapshot,
  getWarnings: mocks.getWarnings,
  getTraceItems: mocks.getTraceItems,
}));

vi.mock('@/features/cache-invalidation', () => ({
  invalidatePostJobProjectionQueries: vi.fn(),
}));

import { useJobsSnapshot, useWarnings, useTraceItems } from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('jobs hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe('useJobsSnapshot', () => {
    it('fetches jobs snapshot from the API', async () => {
      const jobs = [
        { id: 'job-1', status: 'completed', label: 'Import', progress: 100, startedAt: '2026-06-01T10:00:00Z' },
      ];
      mocks.getJobsSnapshot.mockResolvedValue(jobs);

      const { result } = renderHook(() => useJobsSnapshot(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data).toEqual(jobs);
      expect(mocks.getJobsSnapshot).toHaveBeenCalledTimes(1);
    });

    it('returns empty array when no jobs exist', async () => {
      mocks.getJobsSnapshot.mockResolvedValue([]);

      const { result } = renderHook(() => useJobsSnapshot(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data).toEqual([]);
    });
  });

  describe('useWarnings', () => {
    it('fetches warnings from the API', async () => {
      const warnings = [{ id: 'w-1', severity: 'warning', message: 'Low disk space' }];
      mocks.getWarnings.mockResolvedValue(warnings);

      const { result } = renderHook(() => useWarnings(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data).toEqual(warnings);
      expect(mocks.getWarnings).toHaveBeenCalledTimes(1);
    });
  });

  describe('useTraceItems', () => {
    it('fetches trace items from the API', async () => {
      const traces = [{ id: 't-1', kind: 'info', message: 'Import started', timestamp: '2026-06-01T10:00:00Z' }];
      mocks.getTraceItems.mockResolvedValue(traces);

      const { result } = renderHook(() => useTraceItems(), {
        wrapper: createWrapper(),
      });

      await waitFor(() => expect(result.current.isSuccess).toBe(true));
      expect(result.current.data).toEqual(traces);
      expect(mocks.getTraceItems).toHaveBeenCalledTimes(1);
    });
  });
});
