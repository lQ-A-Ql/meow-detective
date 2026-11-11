import { fireEvent, render, screen } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { GraphNodeDetails } from '@/features/graph/components/GraphNodeDetails';
import type { GraphEdge, GraphNode } from '@/types/models';

function makeNode(overrides: Partial<GraphNode> = {}): GraphNode {
  return {
    id: 'node:file-1',
    caseId: 'case-1',
    nodeType: 'file',
    label: 'evidence.doc',
    summary: 'Extracted from the NTFS volume.',
    tags: [],
    createdAt: '2026-06-14T00:00:00Z',
    ...overrides,
  };
}

function makeEdge(id: string, sourceId: string, targetId: string): GraphEdge {
  return {
    id,
    caseId: 'case-1',
    sourceId,
    targetId,
    edgeType: 'references',
    createdAt: '2026-06-14T00:00:00Z',
  };
}

describe('GraphNodeDetails', () => {
  it('renders the node label, type, id, summary, and degree', () => {
    const node = makeNode();
    const edges = [
      makeEdge('e1', node.id, 'node:other'),
      makeEdge('e2', 'node:other', node.id),
      makeEdge('e3', 'node:other', 'node:third'),
    ];
    render(<GraphNodeDetails node={node} edges={edges} onExpand={vi.fn()} onClose={vi.fn()} />);

    expect(screen.getByText('evidence.doc')).toBeInTheDocument();
    expect(screen.getByText(node.id)).toBeInTheDocument();
    expect(screen.getByText(node.summary)).toBeInTheDocument();
    expect(screen.getByText('类型')).toBeInTheDocument();
    expect(screen.getByText('文件')).toBeInTheDocument();
    expect(screen.getByText('度').nextElementSibling).toHaveTextContent('2');
  });

  it('translates every known node type', () => {
    const types = [
      ['artifact', '痕迹'],
      ['timelineEvent', '时间线'],
      ['entity', '实体'],
      ['lead', '线索'],
      ['notebookEntry', '笔记'],
    ] as const;

    for (const [nodeType, label] of types) {
      const { unmount } = render(
        <GraphNodeDetails node={makeNode({ nodeType })} edges={[]} onExpand={vi.fn()} onClose={vi.fn()} />,
      );
      expect(screen.getByText(label)).toBeInTheDocument();
      unmount();
    }
  });

  it('falls back for missing optional metadata and unknown node types', () => {
    render(
      <GraphNodeDetails
        node={makeNode({ label: '', summary: '', nodeType: 'mystery' as GraphNode['nodeType'] })}
        edges={[]}
        onExpand={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByText('无标签')).toBeInTheDocument();
    expect(screen.getByText('无摘要')).toBeInTheDocument();
    expect(screen.getByText('未知')).toBeInTheDocument();
    expect(screen.getByText('度').nextElementSibling).toHaveTextContent('0');
  });

  it('expands one or two levels and closes through the action buttons', () => {
    const onExpand = vi.fn();
    const onClose = vi.fn();
    render(<GraphNodeDetails node={makeNode()} edges={[]} onExpand={onExpand} onClose={onClose} />);

    fireEvent.click(screen.getByRole('button', { name: '展开 1 层' }));
    fireEvent.click(screen.getByRole('button', { name: '展开 2 层' }));
    fireEvent.click(screen.getByRole('button', { name: '关闭' }));

    expect(onExpand).toHaveBeenNthCalledWith(1, 1);
    expect(onExpand).toHaveBeenNthCalledWith(2, 2);
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
