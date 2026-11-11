import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, renderHook, waitFor } from '@testing-library/react';
import { createElement, type ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useLinuxClusterWorkspaceModel } from '@/features/infrastructure/model/use-linux-cluster-workspace-model';
import type {
  LinuxEvidenceSetListItem,
  LinuxEvidenceSetMemberSummary,
  LinuxEvidenceSetSummary,
} from '@/types/linuxCluster';

const mocks = vi.hoisted(() => ({
  currentCase: vi.fn(),
  dataSources: vi.fn(),
  graph: vi.fn(),
  listSets: vi.fn(),
  setSummary: vi.fn(),
  setEvents: vi.fn(),
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
  listLinuxEvidenceSets: mocks.listSets,
  getLinuxEvidenceSetSummary: mocks.setSummary,
  getLinuxEvidenceEvents: mocks.setEvents,
  getLinuxArtifactSummary: mocks.artifactSummary,
}));

function queryResult(data: unknown) {
  return { data, error: null, isFetching: false, isLoading: false, isSuccess: true };
}

const listItems: LinuxEvidenceSetListItem[] = [
  { importSetId: 'set-1', name: 'PVE cluster', state: 'ready', memberCount: 3, readyCount: 3, failedCount: 0 },
  { importSetId: 'set-2', name: 'Second set', state: 'partial', memberCount: 1, readyCount: 0, failedCount: 0 },
];

const member = (overrides: Partial<LinuxEvidenceSetMemberSummary> = {}): LinuxEvidenceSetMemberSummary => ({
  memberIndex: 0,
  dataSourceId: 'source-1',
  sourceName: 'node-a',
  sourcePath: 'node-a.E01',
  sourceKind: 'e01',
  importState: 'ready',
  hashStatus: 'hashed',
  provenanceStatus: 'complete',
  addresses: [],
  roles: [],
  services: [],
  containers: [],
  diagnostics: [],
  ...overrides,
});

const summary = (overrides: Partial<LinuxEvidenceSetSummary> = {}): LinuxEvidenceSetSummary => ({
  importSetId: 'set-1',
  name: 'PVE cluster',
  state: 'ready',
  memberCount: 3,
  readyCount: 3,
  failedCount: 0,
  capabilityLevel: 'metadata_only',
  manifestDigest: 'sha256:abc',
  members: [
    member({ memberIndex: 0, dataSourceId: 'source-1', sourceName: 'node-a' }),
    member({ memberIndex: 1, dataSourceId: null, sourceName: 'node-b' }),
    member({ memberIndex: 2, dataSourceId: 'source-missing', sourceName: 'node-c' }),
  ],
  scopes: [{
    id: 'scope:host',
    kind: 'physical_host',
    name: 'node-a',
    identityState: 'verified',
    status: 'ready',
    evidenceCompleteness: 'complete',
    memberCount: 1,
    memberSourceIds: ['source-1'],
    memberRoles: [],
    diagnostics: [],
  }],
  edges: [],
  derivedSources: [],
  diagnostics: [],
  ...overrides,
});

function renderModel() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const wrapper = ({ children }: { children: ReactNode }) =>
    createElement(QueryClientProvider, { client: queryClient }, children);
  return { ...renderHook(() => useLinuxClusterWorkspaceModel(), { wrapper }), queryClient };
}

describe('useLinuxClusterWorkspaceModel', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    mocks.currentCase.mockReturnValue(queryResult({ id: 'case-1', name: 'cluster case' }));
    mocks.dataSources.mockReturnValue(queryResult([
      { id: 'source-1', name: 'node-a image', fileCount: 10, evidenceSize: 1024, platform: 'linux', importState: 'ready' },
      { id: 'source-2', name: 'node-b image', fileCount: 5, evidenceSize: 512, platform: 'linux', importState: 'ready' },
    ]));
    mocks.graph.mockResolvedValue({ nodes: [], edges: [] });
    mocks.artifactSummary.mockResolvedValue({});
    mocks.listSets.mockResolvedValue(listItems);
    mocks.setSummary.mockResolvedValue(summary());
    mocks.setEvents.mockResolvedValue([]);
  });

  it('auto-selects the first evidence set, loads its details and aggregates only traceable sources', async () => {
    const { result } = renderModel();

    await waitFor(() => expect(result.current.selectedSetId).toBe('set-1'));
    await waitFor(() => expect(result.current.summary.data?.importSetId).toBe('set-1'));
    expect(mocks.setSummary).toHaveBeenCalledWith('set-1');
    expect(mocks.setEvents).toHaveBeenCalledWith('set-1', 0, 100);

    await waitFor(() => expect(result.current.evidenceStats).toEqual({ fileCount: 10, evidenceSize: 1024 }));
    expect(result.current.sourceMetadata.size).toBe(2);
    expect(result.current.sourceMetadata.get('source-1')?.name).toBe('node-a image');
    expect(result.current.capabilities).toHaveLength(6);
    expect(result.current.capabilities.map((item) => item.kind)).toContain('linux_host');
    expect(result.current.provenance).toBe('complete');
    expect(result.current.fallback).toBe(false);

    expect(result.current.section).toBe('overview');
    act(() => result.current.setSection('members'));
    expect(result.current.section).toBe('members');

    expect(result.current.selectedMember).toBeUndefined();
    act(() => result.current.setSelectedMemberIndex(1));
    expect(result.current.selectedMember?.sourceName).toBe('node-b');
    act(() => result.current.setSelectedMemberIndex(99));
    expect(result.current.selectedMember).toBeUndefined();
  });

  it('requests the summary for a manually selected evidence set', async () => {
    const { result } = renderModel();
    await waitFor(() => expect(result.current.selectedSetId).toBe('set-1'));

    act(() => result.current.setSelectedSetId('set-2'));

    await waitFor(() => expect(mocks.setSummary).toHaveBeenCalledWith('set-2'));
    expect(result.current.selectedSetId).toBe('set-2');
  });

  it('falls back without detail queries when no evidence set exists', async () => {
    mocks.listSets.mockResolvedValue([]);
    const { result } = renderModel();

    await waitFor(() => expect(result.current.fallback).toBe(true));
    expect(result.current.selectedSetId).toBeUndefined();
    expect(result.current.capabilities).toEqual([]);
    expect(result.current.provenance).toBe('partial');
    expect(result.current.evidenceStats).toEqual({ fileCount: 0, evidenceSize: 0 });
    expect(result.current.selectedMember).toBeUndefined();
    expect(mocks.setSummary).not.toHaveBeenCalled();
    expect(mocks.setEvents).not.toHaveBeenCalled();
  });

  it('does not report the fallback state while the evidence set list is loading', () => {
    mocks.listSets.mockReturnValue(new Promise(() => undefined));
    const { result } = renderModel();

    expect(result.current.evidenceSets.isLoading).toBe(true);
    expect(result.current.fallback).toBe(false);
  });

  it('does not request evidence sets without an active case', () => {
    mocks.currentCase.mockReturnValue(queryResult(undefined));
    const { result } = renderModel();

    expect(mocks.listSets).not.toHaveBeenCalled();
    expect(result.current.fallback).toBe(true);
  });

  it('computes zero evidence stats when data source metadata is unavailable', async () => {
    mocks.dataSources.mockReturnValue(queryResult(undefined));
    const { result } = renderModel();

    await waitFor(() => expect(result.current.summary.data).toBeDefined());
    expect(result.current.sourceMetadata.size).toBe(0);
    expect(result.current.evidenceStats).toEqual({ fileCount: 0, evidenceSize: 0 });
  });

  it('polls the summary only while a member hash is pending', async () => {
    const { result, queryClient } = renderModel();
    await waitFor(() => expect(result.current.selectedSetId).toBe('set-1'));

    const queryKey = ['linux-evidence-set-summary', 'case-1', 'set-1'];
    await waitFor(() => expect(queryClient.getQueryCache().find({ queryKey })).toBeDefined());
    const query = queryClient.getQueryCache().find({ queryKey })!;
    const interval = (query.options as unknown as {
      refetchInterval?: (value: { state: { data?: LinuxEvidenceSetSummary } }) => number | false | undefined;
    }).refetchInterval;
    expect(interval).toBeTypeOf('function');

    expect(interval!({ state: { data: summary({ members: [member({ hashStatus: 'pending' })] }) } })).toBe(2_000);
    expect(interval!({ state: { data: summary() } })).toBe(false);
    expect(interval!({ state: { data: undefined } })).toBe(false);
  });
});
