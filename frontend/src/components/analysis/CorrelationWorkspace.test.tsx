import { createElement } from 'react';
import { MemoryRouter } from 'react-router';
import { render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { CorrelationWorkspace } from './CorrelationWorkspace';
import type { CorrelationSnapshot } from '@/types/models';

vi.mock('@/stores/selection-store', () => ({
  useSelectionStore: vi.fn().mockReturnValue(vi.fn()),
}));

function makeSnapshot(overrides: Partial<CorrelationSnapshot> = {}): CorrelationSnapshot {
  return {
    leadCount: 0,
    clusterCount: 0,
    nodeCount: 0,
    edgeCount: 0,
    generatedAt: '2026-06-01T10:00:00Z',
    leads: [],
    clusters: [],
    nodes: [],
    edges: [],
    familyCoverage: [],
    ...overrides,
  };
}

function renderWorkspace(snapshot: CorrelationSnapshot) {
  return render(
    createElement(
      MemoryRouter,
      {},
      createElement(CorrelationWorkspace, { snapshot }),
    ),
  );
}

describe('CorrelationWorkspace', () => {
  it('renders empty state when snapshot has no leads', () => {
    renderWorkspace(makeSnapshot());
    expect(screen.getByText('关联分析工作台')).toBeDefined();
    expect(screen.getByText('当前筛选条件下没有可展示的关联线索。')).toBeDefined();
    expect(screen.getByText('当前筛选条件下没有可展示的聚合 cluster。')).toBeDefined();
    expect(screen.getByText('当前暂无可展开的 lead 明细。')).toBeDefined();
  });

  it('renders lead cards when leads exist', () => {
    const snapshot = makeSnapshot({
      leadCount: 2,
      nodeCount: 3,
      edgeCount: 1,
      leads: [
        {
          id: 'lead-1',
          title: 'File access pattern',
          summary: 'User accessed multiple sensitive files',
          primaryFileId: 'file-1',
          supportingNodeIds: ['node-1'],
          confidence: 'strong',
          families: ['FileAccess'],
          matchSignals: ['path match'],
          caveats: [],
          provenance: [],
          jumps: [],
        },
        {
          id: 'lead-2',
          title: 'Browser artifact match',
          summary: 'Browser history shows visits to suspicious sites',
          primaryFileId: 'file-2',
          supportingNodeIds: ['node-2'],
          confidence: 'weak',
          families: ['BrowserHistory'],
          matchSignals: [],
          caveats: ['needs review'],
          provenance: [],
          jumps: [],
        },
      ],
      nodes: [
        {
          id: 'node-1',
          kind: 'file',
          title: 'evidence.txt',
          subtitle: '/path/to/evidence.txt',
          sourceObjectId: 'file-1',
          relatedCount: 0,
          badges: [],
          jumps: [],
        },
        {
          id: 'node-2',
          kind: 'artifact',
          title: 'Chrome history',
          subtitle: 'History DB',
          sourceObjectId: 'file-2',
          relatedCount: 0,
          badges: [],
          jumps: [],
        },
      ],
      edges: [],
      familyCoverage: [
        {
          family: 'FileAccess',
          displayName: 'File Access',
          status: 'covered',
          leadCount: 1,
          highConfidenceLeadCount: 1,
          reviewLeadCount: 0,
          clusterCount: 0,
          sampleSignals: ['path match'],
        },
        {
          family: 'BrowserHistory',
          displayName: 'Browser History',
          status: 'review',
          leadCount: 1,
          highConfidenceLeadCount: 0,
          reviewLeadCount: 1,
          clusterCount: 0,
          sampleSignals: [],
        },
      ],
    });
    renderWorkspace(snapshot);

    // Lead titles may appear in both lead list and detail panel
    expect(screen.getAllByText('File access pattern').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Browser artifact match').length).toBeGreaterThan(0);
    expect(screen.getByText('显示 2 / 2 条线索')).toBeDefined();
    expect(screen.getByText('规则家族覆盖')).toBeDefined();
    expect(screen.getAllByText('File Access').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Browser History').length).toBeGreaterThan(0);
  });

  it('renders cluster cards when clusters exist', () => {
    const snapshot = makeSnapshot({
      leadCount: 1,
      clusterCount: 1,
      leads: [
        {
          id: 'lead-1',
          title: 'Lead A',
          summary: 'Summary A',
          primaryFileId: 'file-1',
          supportingNodeIds: [],
          confidence: 'direct',
          families: [],
          matchSignals: [],
          caveats: [],
          provenance: [],
          jumps: [],
        },
      ],
      nodes: [],
      edges: [],
      clusters: [
        {
          id: 'cluster-1',
          title: 'Evidence Cluster',
          summary: 'Grouped artifacts',
          primaryFileId: 'file-1',
          confidence: 'direct',
          families: ['Registry'],
          artifactCount: 5,
          timelineCount: 3,
          nodeIds: ['n1'],
          edgeIds: ['e1'],
          provenance: [],
        },
      ],
      familyCoverage: [],
    });
    renderWorkspace(snapshot);

    // Cluster title may appear in both cluster list and lead detail
    expect(screen.getAllByText('Evidence Cluster').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Grouped artifacts').length).toBeGreaterThan(0);
  });

  it('shows overview cards and family coverage', () => {
    const snapshot = makeSnapshot({
      leadCount: 5,
      clusterCount: 2,
      nodeCount: 10,
      edgeCount: 8,
      familyCoverage: [
        {
          family: 'Registry',
          displayName: 'Registry',
          status: 'covered',
          leadCount: 3,
          highConfidenceLeadCount: 2,
          reviewLeadCount: 1,
          clusterCount: 1,
          sampleSignals: [],
        },
      ],
    });
    renderWorkspace(snapshot);

    expect(screen.getByText('关联分析工作台')).toBeDefined();
    expect(screen.getByText('规则家族覆盖')).toBeDefined();
    expect(screen.getAllByText('Lead').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Cluster').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Node').length).toBeGreaterThan(0);
    expect(screen.getAllByText('Edge').length).toBeGreaterThan(0);
  });
});
