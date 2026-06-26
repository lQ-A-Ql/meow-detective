import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getArtifactById: vi.fn(),
  getArtifactFamilies: vi.fn(),
  getArtifactRows: vi.fn(),
  getArtifactFamilyCounts: vi.fn(),
}));

vi.mock('@/lib/api/artifacts', () => ({
  getArtifactById: mocks.getArtifactById,
  getArtifactFamilies: mocks.getArtifactFamilies,
  getArtifactRows: mocks.getArtifactRows,
  getArtifactFamilyCounts: mocks.getArtifactFamilyCounts,
}));

import {
  useArtifactById,
  useArtifactFamilies,
  useArtifactFamilyCounts,
  useArtifactRows,
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
