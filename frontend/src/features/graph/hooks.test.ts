import { createElement } from 'react';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({
  getGraphSnapshot: vi.fn(),
  getNodeNeighborhood: vi.fn(),
  queryGraph: vi.fn(),
}));

vi.mock('@/lib/api/graph', () => ({
  getGraphSnapshot: mocks.getGraphSnapshot,
  getNodeNeighborhood: mocks.getNodeNeighborhood,
  queryGraph: mocks.queryGraph,
}));

import { useGraphQuery, useGraphSnapshot, useNodeNeighborhood } from './hooks';

function createWrapper() {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false }, mutations: { retry: false } },
  });
  return function Wrapper({ children }: { children: React.ReactNode }) {
    return createElement(QueryClientProvider, { client: queryClient }, children);
  };
}

describe('graph hooks', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.getGraphSnapshot.mockResolvedValue({
      nodeCountByType: { artifact: 1 },
      edgeCountByType: { linked: 1 },
      totalNodes: 1,
      totalEdges: 1,
      density: 0.5,
      largestComponentSize: 2,
    });
    mocks.getNodeNeighborhood.mockResolvedValue({
      center: 'n1',
      nodes: [{ id: 'n1', kind: 'artifact' }],
      edges: [],
    });
    mocks.queryGraph.mockResolvedValue({
      nodes: [{ id: 'n1' }],
      edges: [],
    });
  });

  it('fetches graph snapshot when caseId is provided', async () => {
    const { result } = renderHook(() => useGraphSnapshot('case-1'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getGraphSnapshot).toHaveBeenCalledTimes(1);
    expect(result.current.data).toBeDefined();
    expect(result.current.data!.totalNodes).toBe(1);
  });

  it('does not fetch graph snapshot when caseId is empty', () => {
    const { result } = renderHook(() => useGraphSnapshot(''), {
      wrapper: createWrapper(),
    });

    expect(result.current.fetchStatus).toBe('idle');
    expect(mocks.getGraphSnapshot).not.toHaveBeenCalled();
  });

  it('fetches graph query results when startIds are provided', async () => {
    const query = { startIds: ['n1'], maxDepth: 2, edgeKinds: ['linked'] };
    const { result } = renderHook(() => useGraphQuery(query as never), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.queryGraph).toHaveBeenCalledWith(query);
  });

  it('does not fetch graph query when startIds is empty', () => {
    const query = { startIds: [], maxDepth: 2, edgeKinds: [] };
    const { result } = renderHook(() => useGraphQuery(query as never), {
      wrapper: createWrapper(),
    });

    expect(result.current.fetchStatus).toBe('idle');
    expect(mocks.queryGraph).not.toHaveBeenCalled();
  });

  it('fetches node neighborhood with default depth', async () => {
    const { result } = renderHook(() => useNodeNeighborhood('n1'), {
      wrapper: createWrapper(),
    });

    await waitFor(() => expect(result.current.isSuccess).toBe(true));
    expect(mocks.getNodeNeighborhood).toHaveBeenCalledWith('n1', 1);
  });
});
