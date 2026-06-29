import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  createBatchPlan: vi.fn(),
  startBatch: vi.fn(),
  pauseBatch: vi.fn(),
  resumeBatch: vi.fn(),
  cancelBatch: vi.fn(),
  getBatchJob: vi.fn(),
  listBatchJobs: vi.fn(),
}));

vi.mock('@/lib/api/batch', () => ({
  createBatchPlan: mocks.createBatchPlan,
  startBatch: mocks.startBatch,
  pauseBatch: mocks.pauseBatch,
  resumeBatch: mocks.resumeBatch,
  cancelBatch: mocks.cancelBatch,
  getBatchJob: mocks.getBatchJob,
  listBatchJobs: mocks.listBatchJobs,
}));

import {
  useBatchJob,
  useCreateBatchPlan,
  useListBatchJobs,
  useStartBatch,
} from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('batch hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.listBatchJobs.mockResolvedValue([
      { id: 'job-1', status: 'completed', plan: {} },
    ]);
    mocks.getBatchJob.mockResolvedValue({
      id: 'job-1',
      status: 'completed',
      plan: {},
    });
    mocks.createBatchPlan.mockResolvedValue({ id: 'job-2', status: 'pending', plan: {} });
    mocks.startBatch.mockResolvedValue({ id: 'job-1', status: 'running' });
    mocks.pauseBatch.mockResolvedValue({ id: 'job-1', status: 'paused' });
    mocks.resumeBatch.mockResolvedValue({ id: 'job-1', status: 'running' });
    mocks.cancelBatch.mockResolvedValue({ id: 'job-1', status: 'cancelled' });
  });

  it('fetches batch job list', async () => {
    const { result } = renderHook(() => useListBatchJobs(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.listBatchJobs).toHaveBeenCalledTimes(1);
    expect(result.current.data).toHaveLength(1);
  });

  it('fetches a single batch job when jobId is provided', async () => {
    const { result } = renderHook(() => useBatchJob('job-1'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getBatchJob).toHaveBeenCalledWith('job-1');
  });

  it('does not fetch batch job when jobId is null', () => {
    const { result } = renderHook(() => useBatchJob(null), {
      wrapper: createWrapper(),
    });

    expect(result.current.fetchStatus).toBe('idle');
    expect(mocks.getBatchJob).not.toHaveBeenCalled();
  });

  it('creates a batch plan and invalidates list queries', async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');

    const wrapper = function Wrapper({ children }: { children: React.ReactNode }) {
      return createElement(QueryClientProvider, { client: qc }, children);
    };

    const { result } = renderHook(() => useCreateBatchPlan(), { wrapper });

    await result.current.mutateAsync({
      items: [{ dataSourceId: 'ds-1', actions: ['hash'] }],
    } as never);

    expect(mocks.createBatchPlan).toHaveBeenCalledTimes(1);
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['batch', 'list'] });
  });

  it('starts a batch job and invalidates batch queries', async () => {
    const qc = new QueryClient({
      defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
    });
    const invalidateSpy = vi.spyOn(qc, 'invalidateQueries');

    const wrapper = function Wrapper({ children }: { children: React.ReactNode }) {
      return createElement(QueryClientProvider, { client: qc }, children);
    };

    const { result } = renderHook(() => useStartBatch(), { wrapper });

    await result.current.mutateAsync('job-1');
    expect(mocks.startBatch).toHaveBeenCalledWith('job-1');
    expect(invalidateSpy).toHaveBeenCalledWith({ queryKey: ['batch'] });
  });
});
