import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { GraphEdgeDetails } from '@/features/graph/components/GraphEdgeDetails';
import type { GraphEdge, GraphNode, GraphProvenanceEntry } from '@/types/models';

function makeEdge(overrides: Partial<GraphEdge> = {}): GraphEdge {
  return {
    id: 'edge-1',
    caseId: 'case-1',
    sourceId: 'node:a',
    targetId: 'node:b',
    edgeType: 'correlatesWith',
    createdAt: '2026-06-14T00:00:00Z',
    ...overrides,
  };
}

function makeNode(id: string, label: string): GraphNode {
  return {
    id,
    caseId: 'case-1',
    nodeType: 'artifact',
    label,
    summary: '',
    tags: [],
    createdAt: '2026-06-14T00:00:00Z',
  };
}

const nodeMap = new Map<string, GraphNode>([
  ['node:a', makeNode('node:a', 'Prefetch artifact')],
  ['node:b', makeNode('node:b', 'Timeline event')],
]);

describe('GraphEdgeDetails', () => {
  it('renders the edge type, confidence, and resolved endpoint labels', () => {
    render(
      <GraphEdgeDetails
        edge={makeEdge({ confidence: 0.82 })}
        nodeMap={nodeMap}
        provenance={[]}
        provenanceLoading={false}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('关联')).toBeInTheDocument();
    expect(screen.getByText('置信度')).toBeInTheDocument();
    expect(screen.getByText('0.82')).toBeInTheDocument();
    expect(screen.getByText('Prefetch artifact')).toBeInTheDocument();
    expect(screen.getByText('Timeline event')).toBeInTheDocument();
  });

  it('falls back to endpoint ids and a dash when metadata is missing', () => {
    render(
      <GraphEdgeDetails
        edge={makeEdge({ sourceId: 'node:unknown-a', targetId: 'node:unknown-b' })}
        nodeMap={nodeMap}
        provenance={undefined}
        provenanceLoading={false}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('node:unknown-a')).toBeInTheDocument();
    expect(screen.getByText('node:unknown-b')).toBeInTheDocument();
    expect(screen.getByText('-')).toBeInTheDocument();
    expect(screen.getByText('无追溯记录')).toBeInTheDocument();
  });

  it('shows a loading state instead of the empty provenance notice', () => {
    render(
      <GraphEdgeDetails
        edge={makeEdge()}
        nodeMap={nodeMap}
        provenance={undefined}
        provenanceLoading
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('加载图数据...')).toBeInTheDocument();
    expect(screen.queryByText('无追溯记录')).toBeNull();
  });

  it('renders only the provenance fields that are present', () => {
    const provenance: GraphProvenanceEntry[] = [
      {
        edgeId: 'edge-1',
        sourceParser: 'evtx',
        sourceRuleId: 'rule-7',
        parserVersion: '1.4.2',
        extractionTimestamp: '2026-06-14T00:00:00Z',
      },
      {
        edgeId: 'edge-1',
        parserVersion: '2.0.0',
      },
    ];
    const { container } = render(
      <GraphEdgeDetails
        edge={makeEdge()}
        nodeMap={nodeMap}
        provenance={provenance}
        provenanceLoading={false}
        onClose={vi.fn()}
      />,
    );

    const text = container.textContent ?? '';
    expect(text).toContain('解析器: evtx');
    expect(text).toContain('规则: rule-7');
    expect(text).toContain('版本: 1.4.2');
    expect(text).toContain('版本: 2.0.0');
    expect(screen.queryByText('无追溯记录')).toBeNull();
    expect(container.querySelectorAll('[class*="p-1.5"]').length).toBeGreaterThanOrEqual(2);
  });

  it('closes through the action button', () => {
    const onClose = vi.fn();
    render(
      <GraphEdgeDetails
        edge={makeEdge()}
        nodeMap={nodeMap}
        provenance={[]}
        provenanceLoading={false}
        onClose={onClose}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: '关闭' }));

    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
