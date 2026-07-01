import { describe, expect, it } from 'vitest';
import { degreeMap, edgeTypeColor, fitTransform, nodeTypeColor, tickSimulation } from './graph-utils';
import type { GraphEdge, GraphNode, NodeType } from '@/types/models';

function makeNode(id: string, type: NodeType = 'file'): GraphNode {
  return {
    id,
    caseId: 'case-1',
    nodeType: type,
    label: id,
    summary: '',
    tags: [],
    createdAt: '2026-06-14T00:00:00Z',
  };
}

function makeEdge(id: string, source: string, target: string): GraphEdge {
  return {
    id,
    caseId: 'case-1',
    sourceId: source,
    targetId: target,
    edgeType: 'references',
    createdAt: '2026-06-14T00:00:00Z',
  };
}

describe('graph-utils', () => {
  it('maps node types to colors', () => {
    expect(nodeTypeColor('file')).not.toBeUndefined();
    expect(nodeTypeColor('artifact')).not.toBeUndefined();
    expect(nodeTypeColor('timelineEvent')).not.toBeUndefined();
  });

  it('maps edge types to colors', () => {
    expect(edgeTypeColor('contains')).not.toBeUndefined();
    expect(edgeTypeColor('correlatesWith')).not.toBeUndefined();
  });

  it('computes node degrees', () => {
    const nodes = [makeNode('a'), makeNode('b'), makeNode('c')];
    const edges = [makeEdge('e1', 'a', 'b'), makeEdge('e2', 'a', 'c')];
    const degrees = degreeMap(nodes, edges);
    expect(degrees.get('a')).toBe(2);
    expect(degrees.get('b')).toBe(1);
    expect(degrees.get('c')).toBe(1);
  });

  it('moves connected nodes closer during simulation', () => {
    const positions = new Map([
      ['a', { id: 'a', x: 0, y: 0, vx: 0, vy: 0, radius: 6 }],
      ['b', { id: 'b', x: 1000, y: 0, vx: 0, vy: 0, radius: 6 }],
    ]);
    const edges = [makeEdge('e1', 'a', 'b')];

    for (let i = 0; i < 200; i += 1) {
      tickSimulation(positions, edges, 800, 600);
    }

    const a = positions.get('a')!;
    const b = positions.get('b')!;
    const distance = Math.abs(b.x - a.x);
    expect(distance).toBeLessThan(500);
  });

  it('fits transform to node bounding box', () => {
    const positions = new Map([
      ['a', { id: 'a', x: 0, y: 0, vx: 0, vy: 0, radius: 6 }],
      ['b', { id: 'b', x: 200, y: 100, vx: 0, vy: 0, radius: 6 }],
    ]);
    const transform = fitTransform(positions, 800, 600);
    expect(transform.k).toBeGreaterThan(0);
    expect(transform.k).toBeLessThanOrEqual(2);
  });

  it('returns identity transform for empty positions', () => {
    const transform = fitTransform(new Map(), 800, 600);
    expect(transform).toEqual({ x: 0, y: 0, k: 1 });
  });
});
