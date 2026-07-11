import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { V3Dashboard } from './V3Dashboard';

const mocks = vi.hoisted(() => ({
  useCurrentCase: vi.fn(),
  useDataSources: vi.fn(),
  useGraphSnapshot: vi.fn(),
  useGraphQuery: vi.fn(),
  useNodeNeighborhood: vi.fn(),
  useProvenanceChain: vi.fn(),
  useFileTree: vi.fn(),
  useTimelineEvents: vi.fn(),
  useArtifactFamilyCounts: vi.fn(),
  useCorrelationSnapshot: vi.fn(),
  useV3GovernanceSnapshot: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.useCurrentCase,
  useDataSources: mocks.useDataSources,
}));

vi.mock('@/features/graph/hooks', () => ({
  useGraphSnapshot: mocks.useGraphSnapshot,
  useGraphQuery: mocks.useGraphQuery,
  useNodeNeighborhood: mocks.useNodeNeighborhood,
  useProvenanceChain: mocks.useProvenanceChain,
}));

vi.mock('@/features/files/hooks', () => ({
  useFileTree: mocks.useFileTree,
}));

vi.mock('@/features/timeline/hooks', () => ({
  useTimelineEvents: mocks.useTimelineEvents,
}));

vi.mock('@/features/artifacts/hooks', () => ({
  useArtifactFamilyCounts: mocks.useArtifactFamilyCounts,
}));

vi.mock('@/features/analysis/hooks', () => ({
  useCorrelationSnapshot: mocks.useCorrelationSnapshot,
  useV3GovernanceSnapshot: mocks.useV3GovernanceSnapshot,
}));

vi.mock('react-router', () => ({
  useNavigate: () => vi.fn(),
}));

function queryState(data: unknown) {
  return {
    data,
    error: null,
    isLoading: false,
    isSuccess: true,
    isFetching: false,
    refetch: vi.fn(),
  };
}

describe('V3Dashboard', () => {
  beforeEach(() => {
    vi.clearAllMocks();

    class MockResizeObserver {
      constructor(private cb: ResizeObserverCallback) {}
      observe(_target: Element) {
        this.cb(
          [{ contentRect: { width: 640, height: 420 } } as unknown as ResizeObserverEntry],
          this as unknown as ResizeObserver,
        );
      }
      unobserve() {}
      disconnect() {}
    }
    global.ResizeObserver = MockResizeObserver as unknown as typeof ResizeObserver;

    global.requestAnimationFrame = vi.fn(() => 0);
    mocks.useCurrentCase.mockReturnValue(queryState({ id: 'case-1' }));
    mocks.useDataSources.mockReturnValue(queryState([
      {
        id: 'ds-1',
        name: 'sample.e01',
        kind: 'e01',
        sourcePath: 'E:/cases/sample.e01',
        importedAt: '2026-06-12T00:00:00Z',
        platform: 'windows',
        fileCount: 1234,
        readerKind: 'ewf',
        partitions: [
          { index: 0, name: 'NTFS', kindLabel: 'Filesystem', status: 'ok', offset: 0, length: 1073741824 },
        ],
      },
      {
        id: 'ds-2',
        name: 'disk.raw',
        kind: 'raw',
        sourcePath: 'E:/cases/disk.raw',
        importedAt: '2026-06-12T00:00:00Z',
        platform: 'windows',
        fileCount: 567,
        readerKind: 'raw',
        partitions: [
          { index: 0, name: 'NTFS', kindLabel: 'Filesystem', status: 'ok', offset: 0, length: 536870912 },
          { index: 1, name: 'FAT32', kindLabel: 'Filesystem', status: 'ok', offset: 536870912, length: 536870912 },
        ],
      },
    ]));
    mocks.useGraphSnapshot.mockReturnValue(queryState({
      nodeCountByType: { file: 150, artifact: 42, timeline: 255 },
      edgeCountByType: { pathMatch: 38, timeCorrelation: 15, parentChild: 147 },
      totalNodes: 392,
      totalEdges: 277,
      density: 0.0026,
      largestComponentSize: 180,
    }));
    mocks.useFileTree.mockReturnValue(queryState([
      { id: 'file-1', name: 'root', path: '/', isDirectory: true, children: [] },
    ]));
    mocks.useGraphQuery.mockReturnValue(queryState({
      nodes: [
        { id: 'file-1', nodeType: 'file', label: 'root', summary: 'Root directory' },
        { id: 'art-1', nodeType: 'artifact', label: 'LNK artifact', summary: '' },
      ],
      edges: [
        { id: 'edge-1', edgeType: 'references', sourceId: 'art-1', targetId: 'file-1', confidence: 0.9, provenance: [] },
      ],
    }));
    mocks.useNodeNeighborhood.mockReturnValue(queryState({ nodes: [], edges: [] }));
    mocks.useProvenanceChain.mockReturnValue(queryState([]));
    mocks.useTimelineEvents.mockReturnValue(queryState({
      total: 3200,
      items: [],
    }));
    mocks.useArtifactFamilyCounts.mockReturnValue(queryState([
      { family: 'LNK', count: 85 },
      { family: 'Prefetch', count: 52 },
      { family: 'EVTX', count: 1200 },
      { family: 'Registry', count: 340 },
    ]));
    mocks.useCorrelationSnapshot.mockReturnValue(queryState({
      generatedAt: '2026-06-12T00:00:00Z',
      nodeCount: 15,
      edgeCount: 8,
      clusterCount: 7,
      leadCount: 9,
      familyCoverage: [
        {
          family: 'LNK',
          displayName: 'LNK',
          status: 'covered',
          leadCount: 1,
          highConfidenceLeadCount: 1,
          reviewLeadCount: 0,
          clusterCount: 1,
          sampleSignals: ['LNK target path matches file path'],
        },
        {
          family: 'Prefetch',
          displayName: 'Prefetch',
          status: 'missing',
          leadCount: 0,
          highConfidenceLeadCount: 0,
          reviewLeadCount: 0,
          clusterCount: 0,
          sampleSignals: [],
        },
      ],
      nodes: [],
      edges: [],
      clusters: [],
      leads: [],
    }));
    mocks.useV3GovernanceSnapshot.mockReturnValue(queryState({
      generatedAt: '2026-06-12T00:00:00Z',
      overallStatus: 'ok',
      rulePackQuality: { coverage: 0, missingRules: 0, staleRules: 0, totalRules: 0 },
      artifactCoverage: { coveredFamilies: 0, missingFamilies: 0, totalFamilies: 0 },
      batchStatus: undefined,
      notebookStats: { entryCount: 0, citationCount: 0 },
      platformCoverage: undefined,
      graphStatistics: { nodeCountByType: {}, edgeCountByType: {}, totalNodes: 0, totalEdges: 0, density: 0, largestComponentSize: 0 },
      rulePackCoverage: undefined,
      familyStatuses: [],
    }));
  });

  it('renders without crashing', () => {
    render(<V3Dashboard />);
    expect(screen.getByText('取证总览')).toBeDefined();
  });

  it('shows graph stats section', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('图统计')).toBeDefined();
    // Stat cards and the embedded graph mini panel share the same snapshot values
    expect(screen.getAllByText('392').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('0.0026').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('180').length).toBeGreaterThanOrEqual(1);

    expect(screen.getByText('按节点类型')).toBeDefined();
    expect(screen.getByText('按边类型')).toBeDefined();
    expect(screen.getByText('file')).toBeDefined();
    expect(screen.getByText('artifact')).toBeDefined();
    expect(screen.getByText('timeline')).toBeDefined();

    expect(screen.getByText('pathMatch')).toBeDefined();
    expect(screen.getByText('timeCorrelation')).toBeDefined();
    expect(screen.getByText('parentChild')).toBeDefined();
  });

  it('shows data source coverage section', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('数据源覆盖')).toBeDefined();
    expect(screen.getByText('源明细')).toBeDefined();
    expect(screen.getByText('sample.e01')).toBeDefined();
    expect(screen.getByText('disk.raw')).toBeDefined();
  });

  it('shows timeline overview section', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('时间线概览')).toBeDefined();
    expect(screen.getByText('3200')).toBeDefined();
  });

  it('shows artifact stats section', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('痕迹统计')).toBeDefined();
    expect(screen.getByText('家族明细')).toBeDefined();
    // LNK appears in both artifact family and correlation family coverage
    expect(screen.getAllByText('LNK').length).toBeGreaterThanOrEqual(1);
    expect(screen.getAllByText('Prefetch').length).toBeGreaterThanOrEqual(1);
  });

  it('shows correlation stats section', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('关联统计')).toBeDefined();
    expect(screen.getByText('家族覆盖')).toBeDefined();
    // Covered/Missing status text appears as direct text nodes in status badges
    expect(screen.getAllByText('covered').length).toBeGreaterThan(0);
    expect(screen.getAllByText('missing').length).toBeGreaterThan(0);
  });

  it('shows placeholder platform coverage section', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('平台覆盖')).toBeDefined();
    expect(screen.getByText('暂无平台覆盖数据。导入数据源并运行痕迹提取后生成。')).toBeDefined();
  });

  it('shows placeholder rule pack status section', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('规则包状态')).toBeDefined();
    expect(screen.getByText('规则包数据将在导入数据源后加载。')).toBeDefined();
  });

  it('shows placeholder batch status section', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('批处理状态')).toBeDefined();
    expect(screen.getByText('暂无批处理作业。')).toBeDefined();
  });
});
