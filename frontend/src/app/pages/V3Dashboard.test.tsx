import { render, screen, within } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { V3Dashboard } from './V3Dashboard';

const mocks = vi.hoisted(() => ({
  useCurrentCase: vi.fn(),
  useCaseOverviewSnapshot: vi.fn(),
  useGraphSnapshot: vi.fn(),
  useGraphQuery: vi.fn(),
  useNodeNeighborhood: vi.fn(),
  useProvenanceChain: vi.fn(),
  useFileTree: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.useCurrentCase,
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

vi.mock('@/features/analysis/hooks', () => ({
  useCaseOverviewSnapshot: mocks.useCaseOverviewSnapshot,
}));

vi.mock('react-router', () => ({
  useNavigate: () => vi.fn(),
}));

function queryState(data: unknown) {
  return {
    data,
    error: null,
    isError: false,
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
    mocks.useCaseOverviewSnapshot.mockReturnValue(queryState({
      generatedAt: '2026-06-12T00:00:00Z',
      dataSources: [
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
      ],
      timelineEventCount: 3200,
      artifactFamilyCounts: [
        { family: 'LNK', count: 85 },
        { family: 'Prefetch', count: 52 },
        { family: 'EVTX', count: 1200 },
        { family: 'Registry', count: 340 },
      ],
      correlationStatistics: {
        nodeCount: 15,
        edgeCount: 8,
        clusterCount: 7,
        leadCount: 9,
        familyCoverage: [
          { family: 'LNK', displayName: 'LNK', status: 'covered', leadCount: 1, highConfidenceLeadCount: 1, reviewLeadCount: 0, clusterCount: 1, sampleSignals: ['LNK target path matches file path'] },
          { family: 'Prefetch', displayName: 'Prefetch', status: 'missing', leadCount: 0, highConfidenceLeadCount: 0, reviewLeadCount: 0, clusterCount: 0, sampleSignals: [] },
        ],
      },
      platformCoverage: {
        windowsArtifactFamilies: 4,
        linuxArtifactFamilies: 0,
        crossPlatformArtifactFamilies: 0,
        unknownArtifactFamilies: 0,
        totalFamilies: 4,
        windowsFamilies: ['LNK', 'Prefetch', 'EVTX', 'Registry'],
        linuxFamilies: [],
        crossPlatformFamilies: [],
        unknownFamilies: [],
      },
      rulePackCoverage: {
        loadedPacks: [{ name: 'v2-standard', version: '1.0.0', author: 'Meow_Detective', ruleCount: 10, scope: ['correlation'] }],
        totalRuleCount: 10,
        loadStatus: 'loaded',
        executionStatus: 'not_executed',
      },
      batchStatus: { activeJobs: 0, completedJobs: 0, failedJobs: 0, queuedJobs: 0, totalJobs: 0 },
    }));
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

    const artifactSection = screen.getByText('痕迹统计').closest('section');
    expect(artifactSection).not.toBeNull();
    expect(within(artifactSection as HTMLElement).getByText('家族明细')).toBeDefined();
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

  it('shows platform coverage from the overview snapshot', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('平台覆盖')).toBeDefined();
    expect(screen.getAllByText('Windows').length).toBeGreaterThan(0);
    expect(screen.getByText('未分类')).toBeDefined();
  });

  it('separates rule definition and case execution state', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('规则包状态')).toBeDefined();
    expect(screen.getByText('定义状态')).toBeDefined();
    expect(screen.getByText('本案执行')).toBeDefined();
    expect(screen.getByText('not_executed')).toBeDefined();
  });

  it('shows an empty but successfully loaded batch status', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('批处理状态')).toBeDefined();
    expect(screen.getByText('进行中')).toBeDefined();
    expect(screen.getByText('排队中')).toBeDefined();
  });

  it('shows overview failures as errors instead of zero-valued facts', () => {
    mocks.useCaseOverviewSnapshot.mockReturnValue({
      ...queryState(undefined),
      error: new Error('overview query failed'),
      isError: true,
      isSuccess: false,
    });

    render(<V3Dashboard />);

    expect(screen.getAllByText('overview query failed').length).toBeGreaterThan(1);
    expect(screen.queryByText('当前案件没有数据源。')).toBeNull();
  });
});
