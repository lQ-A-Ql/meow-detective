import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { createElement, type ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useInfrastructureWorkspace } from '@/features/infrastructure/hooks/useInfrastructureWorkspace';
import type { InfrastructureGraph, InfrastructureGraphNode } from '@/types/models';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  dataSources: vi.fn(),
  graph: vi.fn(),
  artifactSummary: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.currentCase,
  useDataSources: mocks.dataSources,
}));

vi.mock('@/lib/api/infrastructure', () => ({
  getInfrastructureGraph: mocks.graph,
}));

vi.mock('@/lib/api/analysis', () => ({
  getLinuxArtifactSummary: mocks.artifactSummary,
}));

function queryResult(data: unknown) {
  return { data, error: null, isFetching: false, isLoading: false, isSuccess: true };
}

function createWrapper() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return function Wrapper({ children }: { children: ReactNode }) {
    return createElement(QueryClientProvider, { client }, children);
  };
}

const node = (overrides: Partial<InfrastructureGraphNode>): InfrastructureGraphNode => ({
  id: 'node-1',
  domain: 'environment',
  kind: 'physical_host',
  name: 'node-1',
  status: 'ready',
  confidence: 'candidate',
  provenanceJson: '{}',
  ...overrides,
});

const graph: InfrastructureGraph = {
  nodes: [
    node({ id: 'host-1', name: 'host-a', kind: 'physical_host' }),
    node({ id: 'vm-1', name: 'web-01', kind: 'virtual_machine', status: 'partial' }),
    node({ id: 'ceph-1', domain: 'storage', name: 'ceph-cluster', kind: 'ceph_cluster' }),
    node({ id: 'log-1', domain: 'analysis', name: 'audit-log', kind: 'audit_log', status: 'parsed' }),
  ],
  edges: [
    { sourceId: 'host-1', targetId: 'vm-1', relationKind: 'boots', confidence: 'candidate', provenanceJson: '{}' },
    { sourceId: 'vm-1', targetId: 'ceph-1', relationKind: 'consumes_storage', confidence: 'candidate', provenanceJson: '{}' },
    { sourceId: 'log-1', targetId: 'host-1', relationKind: 'derived_from', confidence: 'candidate', provenanceJson: '{}' },
  ],
  networkFacts: [
    {
      id: 'fact-1',
      dataSourceId: 'source-1',
      environmentObjectId: 'host-1',
      fileId: 'f1',
      sourcePath: '/etc/network/interfaces',
      lineNumber: 3,
      factKind: 'interface_address',
      subject: 'vmbr0',
      value: '192.0.2.10/24',
      assertionKind: 'configured',
      confidence: 'candidate',
      parser: 'linux.network.config.v1',
    },
  ],
  networkDiagnostics: ['topology derived from configuration files'],
};

describe('useInfrastructureWorkspace', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.currentCase.mockReturnValue(queryResult({ id: 'case-1', name: 'cluster case' }));
    mocks.dataSources.mockReturnValue(queryResult([]));
    mocks.graph.mockResolvedValue(graph);
  });

  it('exposes domain grouping, node names and the projected network topology', async () => {
    const { result } = renderHook(() => useInfrastructureWorkspace(), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.nodes).toHaveLength(4));
    expect(result.current.edges).toHaveLength(3);
    expect(result.current.domains.environment.map((item) => item.id)).toEqual(['host-1', 'vm-1']);
    expect(result.current.domains.storage.map((item) => item.id)).toEqual(['ceph-1']);
    expect(result.current.domains.analysis.map((item) => item.id)).toEqual(['log-1']);
    expect(result.current.nodeNames.get('host-1')).toBe('host-a');
    expect(result.current.nodeNames.get('missing')).toBeUndefined();

    // Analysis evidence is not painted into the host network view.
    expect(result.current.networkTopology.nodes.map((item) => item.id)).toEqual(['host-1', 'vm-1', 'ceph-1']);
    expect(result.current.networkTopology.edges.map((edge) => edge.relationKind)).toEqual(['boots', 'consumes_storage']);
    expect(result.current.networkFacts).toHaveLength(1);
  });

  it('filters visible nodes by the trimmed, case-insensitive query', async () => {
    const { result } = renderHook(() => useInfrastructureWorkspace(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.visibleNodes).toHaveLength(4));

    act(() => result.current.setQuery('  HOST-a  '));
    expect(result.current.query).toBe('  HOST-a  ');
    expect(result.current.visibleNodes.map((item) => item.id)).toEqual(['host-1']);

    act(() => result.current.setQuery('audit-log'));
    expect(result.current.visibleNodes.map((item) => item.id)).toEqual(['log-1']);

    act(() => result.current.setQuery('does-not-exist'));
    expect(result.current.visibleNodes).toEqual([]);

    act(() => result.current.setQuery(''));
    expect(result.current.visibleNodes).toHaveLength(4);
  });

  it('resolves the selected node and clears the selection for an unknown id', async () => {
    const { result } = renderHook(() => useInfrastructureWorkspace(), { wrapper: createWrapper() });
    await waitFor(() => expect(result.current.nodes).toHaveLength(4));

    expect(result.current.selected).toBeUndefined();
    expect(result.current.selectedId).toBeUndefined();

    act(() => result.current.setSelectedId('vm-1'));
    expect(result.current.selectedId).toBe('vm-1');
    expect(result.current.selected?.name).toBe('web-01');

    act(() => result.current.setSelectedId('ghost'));
    expect(result.current.selectedId).toBe('ghost');
    expect(result.current.selected).toBeUndefined();
  });

  it('defaults network facts to an empty list when the graph omits them', async () => {
    mocks.graph.mockResolvedValue({ nodes: graph.nodes, edges: graph.edges } satisfies InfrastructureGraph);

    const { result } = renderHook(() => useInfrastructureWorkspace(), { wrapper: createWrapper() });

    await waitFor(() => expect(result.current.nodes).toHaveLength(4));
    expect(result.current.networkFacts).toEqual([]);
    expect(result.current.networkTopology.edges).toHaveLength(2);
  });

  it('keeps empty collections while the graph request is still pending', () => {
    mocks.graph.mockReturnValue(new Promise(() => undefined));

    const { result } = renderHook(() => useInfrastructureWorkspace(), { wrapper: createWrapper() });

    expect(result.current.nodes).toEqual([]);
    expect(result.current.edges).toEqual([]);
    expect(result.current.visibleNodes).toEqual([]);
    expect(result.current.networkFacts).toEqual([]);
    expect(result.current.networkTopology).toEqual({ nodes: [], edges: [] });
    expect(result.current.domains.environment).toEqual([]);
  });

  it('does not request a graph without an active case', () => {
    mocks.currentCase.mockReturnValue(queryResult(undefined));

    const { result } = renderHook(() => useInfrastructureWorkspace(), { wrapper: createWrapper() });

    expect(mocks.graph).not.toHaveBeenCalled();
    expect(result.current.nodes).toEqual([]);
    expect(result.current.selected).toBeUndefined();
  });
});
