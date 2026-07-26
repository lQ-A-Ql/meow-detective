import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  searchFiles: vi.fn(),
}));

vi.mock('@/lib/api/search', () => ({
  searchFiles: mocks.searchFiles,
}));

import { useInfiniteSearchResults, useSearchResults } from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('search hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.searchFiles.mockResolvedValue([
      { id: 'f1', name: 'evidence.txt', path: '/data/evidence.txt', score: 0.95 },
      { id: 'f2', name: 'log.txt', path: '/data/log.txt', score: 0.82 },
    ]);
  });

  it('fetches search results for a given query', async () => {
    const { result } = renderHook(() => useSearchResults('evidence'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.searchFiles).toHaveBeenCalledWith('evidence');
    expect(result.current.data).toHaveLength(2);
  });

  it('passes query text to the search API', async () => {
    const { result } = renderHook(() => useSearchResults('malware ransomware'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.searchFiles).toHaveBeenCalledWith('malware ransomware');
  });

  it('returns empty results when query matches nothing', async () => {
    mocks.searchFiles.mockResolvedValue([]);

    const { result } = renderHook(() => useSearchResults('nonexistent'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toEqual([]);
  });

  it('exposes error state when search fails', async () => {
    mocks.searchFiles.mockRejectedValue(new Error('index not ready'));

    const { result } = renderHook(() => useSearchResults('test'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect((result.current.error as Error).message).toBe('index not ready');
  });

  it('does not retry on failure', async () => {
    mocks.searchFiles.mockRejectedValue(new Error('timeout'));

    const { result } = renderHook(() => useSearchResults('timeout test'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect(mocks.searchFiles).toHaveBeenCalledTimes(1);
  });

  it('loads subsequent search cursors without replaying a cumulative offset', async () => {
    mocks.searchFiles
      .mockResolvedValueOnce({
        total: 3,
        available: 3,
        truncated: false,
        items: [{ id: 'f1' }, { id: 'f2' }],
        nextCursor: 'cursor-1',
      })
      .mockResolvedValueOnce({
        total: 3,
        available: 3,
        truncated: false,
        items: [{ id: 'f3' }],
      });

    const { result } = renderHook(() => useInfiniteSearchResults('evidence', 2), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.searchFiles).toHaveBeenNthCalledWith(1, 'evidence', 0, 2, undefined);

    let nextResult: Awaited<ReturnType<typeof result.current.fetchNextPage>> | undefined;
    await act(async () => {
      nextResult = await result.current.fetchNextPage();
    });

    expect(mocks.searchFiles).toHaveBeenNthCalledWith(2, 'evidence', 0, 2, 'cursor-1');
    expect(nextResult?.data?.pages.flatMap((page) => page.items)).toHaveLength(3);
    expect(nextResult?.hasNextPage).toBe(false);
  });

  it('stops search pagination when a stale total yields an empty page', async () => {
    mocks.searchFiles
      .mockResolvedValueOnce({
        total: 3,
        available: 3,
        truncated: false,
        items: [{ id: 'f1' }, { id: 'f2' }],
        nextCursor: 'cursor-1',
      })
      .mockResolvedValueOnce({ total: 3, available: 3, truncated: false, items: [] });
    const { result } = renderHook(() => useInfiniteSearchResults('evidence', 2), {
      wrapper: createWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    await waitFor(() => expect(result.current.hasNextPage).toBe(true));
    await act(async () => {
      await result.current.fetchNextPage();
    });
    await waitFor(() => expect(result.current.hasNextPage).toBe(false));
  });

  it('stops at the backend result window while preserving the real total', async () => {
    mocks.searchFiles.mockResolvedValueOnce({
      total: 200_000,
      available: 2,
      truncated: true,
      items: [{ id: 'f1' }, { id: 'f2' }],
    });
    const { result } = renderHook(() => useInfiniteSearchResults('evidence', 2), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.pages[0]?.total).toBe(200_000);
    expect(result.current.hasNextPage).toBe(false);
    expect(mocks.searchFiles).toHaveBeenCalledTimes(1);
  });
});
