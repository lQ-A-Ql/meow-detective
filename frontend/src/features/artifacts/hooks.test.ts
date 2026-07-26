import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getArtifactById: vi.fn(),
  getArtifactFamilies: vi.fn(),
  getArtifactRows: vi.fn(),
  getArtifactRowsPage: vi.fn(),
  getArtifactFamilyCounts: vi.fn(),
}));

vi.mock('@/lib/api/artifacts', () => ({
  getArtifactById: mocks.getArtifactById,
  getArtifactFamilies: mocks.getArtifactFamilies,
  getArtifactRows: mocks.getArtifactRows,
  getArtifactRowsPage: mocks.getArtifactRowsPage,
  getArtifactFamilyCounts: mocks.getArtifactFamilyCounts,
}));

import {
  useArtifactById,
  useArtifactFamilies,
  useArtifactFamilyCounts,
  useArtifactRows,
  useInfiniteArtifactRows,
} from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('artifacts hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getArtifactFamilies.mockResolvedValue([
      { family: 'Registry', displayName: 'Registry', count: 10 },
    ]);
    mocks.getArtifactRows.mockResolvedValue([
      { id: 'a1', family: 'Registry', source: 'SYSTEM', summary: 'Test artifact' },
    ]);
    mocks.getArtifactRowsPage.mockResolvedValue({
      total: 1,
      items: [{ id: 'a1', family: 'Registry', source: 'SYSTEM', summary: 'Test artifact' }],
    });
    mocks.getArtifactFamilyCounts.mockResolvedValue([
      { family: 'Registry', count: 10 },
    ]);
    mocks.getArtifactById.mockResolvedValue({
      id: 'a1',
      family: 'Registry',
      source: 'SYSTEM',
      summary: 'Test artifact',
    });
  });

  it('fetches artifact families', async () => {
    const { result } = renderHook(() => useArtifactFamilies(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.data).toHaveLength(1);
    expect(mocks.getArtifactFamilies).toHaveBeenCalledTimes(1);
  });

  it('fetches artifact rows for a given family', async () => {
    const { result } = renderHook(() => useArtifactRows('Registry'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getArtifactRows).toHaveBeenCalledWith('Registry');
    expect(result.current.data).toHaveLength(1);
  });

  it('fetches artifact family counts', async () => {
    const { result } = renderHook(() => useArtifactFamilyCounts(), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getArtifactFamilyCounts).toHaveBeenCalledTimes(1);
    expect(result.current.data).toEqual([{ family: 'Registry', count: 10 }]);
  });

  it('loads artifact pages using the backend opaque cursor', async () => {
    mocks.getArtifactRowsPage
      .mockResolvedValueOnce({
        total: 3,
        items: [{ id: 'a1' }, { id: 'a2' }],
        nextCursor: 'artifact-cursor-1',
      })
      .mockResolvedValueOnce({
        total: 3,
        items: [{ id: 'a3' }],
      });

    const { result } = renderHook(() => useInfiniteArtifactRows('Registry', 2), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getArtifactRowsPage).toHaveBeenNthCalledWith(1, 'Registry', undefined, 2);

    let nextResult: Awaited<ReturnType<typeof result.current.fetchNextPage>> | undefined;
    await act(async () => {
      nextResult = await result.current.fetchNextPage();
    });

    expect(mocks.getArtifactRowsPage).toHaveBeenNthCalledWith(
      2,
      'Registry',
      'artifact-cursor-1',
      2,
    );
    expect(nextResult?.data?.pages.flatMap((page) => page.items)).toHaveLength(3);
    expect(nextResult?.hasNextPage).toBe(false);
  });

  it('stops artifact pagination when the backend omits nextCursor', async () => {
    mocks.getArtifactRowsPage.mockResolvedValueOnce({
      total: 3,
      items: [{ id: 'a1' }, { id: 'a2' }],
    });
    const { result } = renderHook(() => useInfiniteArtifactRows('Registry', 2), {
      wrapper: createWrapper(),
    });
    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(result.current.hasNextPage).toBe(false);
    expect(mocks.getArtifactRowsPage).toHaveBeenCalledTimes(1);
  });

  it('fetches artifact by id when id is provided', async () => {
    const { result } = renderHook(() => useArtifactById('a1'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getArtifactById).toHaveBeenCalledWith('a1');
  });

  it('does not fetch artifact by id when id is undefined', () => {
    const { result } = renderHook(() => useArtifactById(undefined), {
      wrapper: createWrapper(),
    });

    expect(result.current.fetchStatus).toBe('idle');
    expect(mocks.getArtifactById).not.toHaveBeenCalled();
  });
});
