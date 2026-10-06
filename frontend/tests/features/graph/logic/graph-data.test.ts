import { describe, expect, it } from 'vitest';
import { MAX_GRAPH_EDGES, MAX_GRAPH_NODES, mergeGraphData, type GraphData } from '@/features/graph/logic/graph-data';
import type { GraphEdge, GraphNode } from '@/types/models';

const node = (id: string) => ({ id } as GraphNode);
const edge = (id: string) => ({ id } as GraphEdge);

describe('graph data limits', () => {
  it('caps new nodes and marks the result truncated', () => {
    const previous: GraphData = { nodes: Array.from({ length: MAX_GRAPH_NODES }, (_, index) => node(`n${index}`)), edges: [], truncated: false };
    const result = mergeGraphData(previous, [node('new-node')], []);

    expect(result.nodes).toHaveLength(MAX_GRAPH_NODES);
    expect(result.truncated).toBe(true);
  });

  it('caps new edges while retaining unique additions below the limit', () => {
    const previous: GraphData = { nodes: [], edges: Array.from({ length: MAX_GRAPH_EDGES - 1 }, (_, index) => edge(`e${index}`)), truncated: false };
    const result = mergeGraphData(previous, [], [edge('new-edge'), edge('overflow-edge')]);

    expect(result.edges).toHaveLength(MAX_GRAPH_EDGES);
    expect(result.edges.at(-1)?.id).toBe('new-edge');
    expect(result.truncated).toBe(true);
  });
});
