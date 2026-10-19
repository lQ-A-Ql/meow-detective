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

import { useInfiniteSearchResults, useSearchResults } from '@/features/search/hooks';

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
    mocks.searchFiles.mockResolvedValue({
      total: 2,
      available: 2,
      truncated: false,
      tookMs: 1,
      items: [],
      coverage: {
        readySourceCount: 1,
        indexedSourceCount: 1,
        expectedEntryCount: 0,
        indexedEntryCount: 0,
        missingSourceIds: [],
        complete: true,
      },
    });
  });

  it('fetches search results for a given query', async () => {
    const { result } = renderHook(() => useSearchResults('case-a', 'evidence'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.searchFiles).toHaveBeenCalledWith('evidence');
    expect(result.current.data?.items).toEqual([]);
  });

  it('passes query text to the search API', async () => {
    const { result } = renderHook(() => useSearchResults('case-a', 'malware ransomware'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.searchFiles).toHaveBeenCalledWith('malware ransomware');
  });

  it('returns empty results when query matches nothing', async () => {
    mocks.searchFiles.mockResolvedValue({
      total: 0,
      available: 0,
      truncated: false,
      tookMs: 0,
      items: [],
      coverage: {
        readySourceCount: 0,
        indexedSourceCount: 0,
        expectedEntryCount: 0,
        indexedEntryCount: 0,
        missingSourceIds: [],
        complete: true,
      },
    });

    const { result } = renderHook(() => useSearchResults('case-a', 'nonexistent'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.items).toEqual([]);
  });

  it('exposes error state when search fails', async () => {
    mocks.searchFiles.mockRejectedValue(new Error('index not ready'));

    const { result } = renderHook(() => useSearchResults('case-a', 'test'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isError).toBe(true));
    expect((result.current.error as Error).message).toBe('index not ready');
  });

  it('does not retry on failure', async () => {
    mocks.searchFiles.mockRejectedValue(new Error('timeout'));

    const { result } = renderHook(() => useSearchResults('case-a', 'timeout test'), {
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
        tookMs: 1,
        items: [{ fileId: 'f1' }, { fileId: 'f2' }],
        coverage: { complete: true },
        nextCursor: 'cursor-1',
      })
      .mockResolvedValueOnce({
        total: 3,
        available: 3,
        truncated: false,
        tookMs: 1,
        items: [{ fileId: 'f3' }],
        coverage: { complete: true },
      });

    const { result } = renderHook(() => useInfiniteSearchResults('case-a', 'evidence', 2), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.searchFiles).toHaveBeenNthCalledWith(
      1,
      'evidence',
      0,
      2,
      undefined,
      expect.objectContaining({ sortKey: 'name', sortDirection: 'asc' }),
    );

    let nextResult: Awaited<ReturnType<typeof result.current.fetchNextPage>> | undefined;
    await act(async () => {
      nextResult = await result.current.fetchNextPage();
    });

    expect(mocks.searchFiles).toHaveBeenNthCalledWith(
      2,
      'evidence',
      0,
      10,
      'cursor-1',
      expect.objectContaining({ sortKey: 'name', sortDirection: 'asc' }),
    );
    expect(nextResult?.data?.pages.flatMap((page) => page.items)).toHaveLength(3);
    expect(nextResult?.hasNextPage).toBe(false);
  });

  it('stops search pagination when a stale total yields an empty page', async () => {
    mocks.searchFiles
      .mockResolvedValueOnce({
        total: 3,
        available: 3,
        truncated: false,
        tookMs: 1,
        items: [{ fileId: 'f1' }, { fileId: 'f2' }],
        coverage: { complete: true },
        nextCursor: 'cursor-1',
      })
      .mockResolvedValueOnce({
        total: 3,
        available: 3,
        truncated: false,
        tookMs: 1,
        items: [],
        coverage: { complete: true },
      });
    const { result } = renderHook(() => useInfiniteSearchResults('case-a', 'evidence', 2), {
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
      tookMs: 1,
      items: [{ fileId: 'f1' }, { fileId: 'f2' }],
      coverage: { complete: true },
    });
    const { result } = renderHook(() => useInfiniteSearchResults('case-a', 'evidence', 2), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.pages[0]?.total).toBe(200_000);
    expect(result.current.hasNextPage).toBe(false);
    expect(mocks.searchFiles).toHaveBeenCalledTimes(1);
  });

  it('browses an empty query only when a case is open', async () => {
    const { result, rerender } = renderHook(({ caseId }) => useInfiniteSearchResults(caseId, ''), {
      initialProps: { caseId: undefined as string | undefined }, wrapper: createWrapper(),
    });
    expect(mocks.searchFiles).not.toHaveBeenCalled();
    rerender({ caseId: 'case-a' });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.searchFiles).toHaveBeenCalledWith('', 0, 100, undefined, expect.any(Object));
  });

  it('keeps identical text in different cases in separate caches', async () => {
    mocks.searchFiles.mockResolvedValueOnce({ total: 1, items: [{ fileId: 'case-a-file' }], tookMs: 0 });
    const { result, rerender } = renderHook(({ caseId }) => useInfiniteSearchResults(caseId, 'same'), {
      initialProps: { caseId: 'case-a' }, wrapper: createWrapper(),
    });
    await waitFor(() => expect(result.current.data?.pages[0]?.items[0].fileId).toBe('case-a-file'));
    mocks.searchFiles.mockResolvedValueOnce({ total: 1, items: [{ fileId: 'case-b-file' }], tookMs: 0 });
    rerender({ caseId: 'case-b' });
    expect(result.current.data).toBeUndefined();
    await waitFor(() => expect(result.current.data?.pages[0]?.items[0].fileId).toBe('case-b-file'));
    expect(mocks.searchFiles).toHaveBeenCalledTimes(2);
  });

  it('bounds loaded rows without changing the actual total or over-fetching the final page', async () => {
    mocks.searchFiles.mockImplementation(async (_query, _offset, limit, cursor) => {
      const start = cursor ? Number(cursor) : 0;
      return { total: 8_000, tookMs: 0, items: Array.from({ length: limit }, (_, index) => ({ fileId: `file-${start+index}` })), nextCursor: String(start+limit) };
    });
    const { result } = renderHook(() => useInfiniteSearchResults('case-a', ''), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data?.pages[0].items).toHaveLength(100);
    expect(result.current.hasNextPage).toBe(true);
    for (let count = 0; count < 10; count++) {
      await act(async () => {
        const page = await result.current.fetchNextPage();
        expect(page.error).toBeNull();
      });
      await waitFor(() => expect(result.current.data?.pages).toHaveLength(count + 2));
    }
    expect(result.current.data?.pages.flatMap((page) => page.items)).toHaveLength(5_000);
    expect(result.current.data?.pages[0]?.total).toBe(8_000);
    expect(result.current.hasNextPage).toBe(false);
    expect(mocks.searchFiles).toHaveBeenLastCalledWith('', 0, 400, '4600', expect.any(Object));
  });
});
