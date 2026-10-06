import { buildEdgeMap, buildNodeMap } from './graph-utils';
import type { GraphEdge, GraphNode } from '@/types/models';

export const MAX_GRAPH_NODES = 500;
export const MAX_GRAPH_EDGES = 2_000;

export type GraphData = {
  nodes: GraphNode[];
  edges: GraphEdge[];
  truncated: boolean;
};

export function mergeGraphData(
  previous: GraphData,
  newNodes: GraphNode[],
  newEdges: GraphEdge[],
): GraphData {
  const nodeMap = buildNodeMap(previous.nodes);
  const edgeMap = buildEdgeMap(previous.edges);
  const nodes = [...previous.nodes];
  const edges = [...previous.edges];
  let truncated = previous.truncated;

  for (const node of newNodes) {
    if (!nodeMap.has(node.id)) {
      if (nodes.length >= MAX_GRAPH_NODES) {
        truncated = true;
        break;
      }
      nodeMap.set(node.id, node);
      nodes.push(node);
    }
  }
  for (const edge of newEdges) {
    if (!edgeMap.has(edge.id)) {
      if (edges.length >= MAX_GRAPH_EDGES) {
        truncated = true;
        break;
      }
      edgeMap.set(edge.id, edge);
      edges.push(edge);
    }
  }
  return { nodes, edges, truncated };
}
