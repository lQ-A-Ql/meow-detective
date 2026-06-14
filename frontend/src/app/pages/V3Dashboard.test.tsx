import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { V3Dashboard } from './V3Dashboard';

const mocks = vi.hoisted(() => ({
  useCurrentCase: vi.fn(),
  useDataSources: vi.fn(),
  useGraphSnapshot: vi.fn(),
  useTimelineEvents: vi.fn(),
  useArtifactFamilyCounts: vi.fn(),
  useCorrelationSnapshot: vi.fn(),
}));

vi.mock('@/features/case/hooks', () => ({
  useCurrentCase: mocks.useCurrentCase,
  useDataSources: mocks.useDataSources,
}));

vi.mock('@/features/graph/hooks', () => ({
  useGraphSnapshot: mocks.useGraphSnapshot,
}));

vi.mock('@/features/timeline/hooks', () => ({
  useTimelineEvents: mocks.useTimelineEvents,
}));

vi.mock('@/features/artifacts/hooks', () => ({
  useArtifactFamilyCounts: mocks.useArtifactFamilyCounts,
}));

vi.mock('@/features/analysis/hooks', () => ({
  useCorrelationSnapshot: mocks.useCorrelationSnapshot,
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
    mocks.useCurrentCase.mockReturnValue(queryState({ id: 'case-1' }));
    mocks.useDataSources.mockReturnValue(queryState([
      {
        id: 'ds-1',
        name: 'sample.e01',
        kind: 'e01',
        sourcePath: 'E:/cases/sample.e01',
        importedAt: '2026-06-12T00:00:00Z',
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
  });

  it('renders without crashing', () => {
    render(<V3Dashboard />);
    expect(screen.getByText('V3 治理台')).toBeDefined();
  });

  it('shows graph stats section', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('图统计')).toBeDefined();
    // Use unique values that do not collide with breakdown list entries
    expect(screen.getByText('392')).toBeDefined();
    expect(screen.getByText('0.0026')).toBeDefined();
    expect(screen.getByText('180')).toBeDefined();

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
    expect(screen.getByText('平台覆盖矩阵将在规则包导入后动态生成。')).toBeDefined();
  });

  it('shows placeholder rule pack status section', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('规则包状态')).toBeDefined();
    expect(screen.getByText('规则包管理将在后续版本中实现。')).toBeDefined();
  });

  it('shows placeholder batch status section', () => {
    render(<V3Dashboard />);

    expect(screen.getByText('批处理状态')).toBeDefined();
    expect(screen.getByText('批处理状态将在后续版本中实现。')).toBeDefined();
  });
});
