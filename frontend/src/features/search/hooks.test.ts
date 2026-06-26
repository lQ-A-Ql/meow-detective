import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  searchFiles: vi.fn(),
}));

vi.mock('@/lib/api/search', () => ({
  searchFiles: mocks.searchFiles,
}));

import { useSearchResults } from './hooks';

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
});
