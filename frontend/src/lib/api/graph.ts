import { GraphProvenanceEntry, GraphQuery, GraphQueryResult, GraphSnapshot } from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function getGraphSnapshot(): Promise<GraphSnapshot> {
  return apiClient.request(COMMANDS.graph.GET_GRAPH_SNAPSHOT, {});
}

export async function queryGraph(query: GraphQuery): Promise<GraphQueryResult> {
  return apiClient.request(COMMANDS.graph.QUERY_GRAPH, { query });
}

export async function getNodeNeighborhood(nodeId: string, depth?: number): Promise<GraphQueryResult> {
  return apiClient.request(COMMANDS.graph.GET_NODE_NEIGHBORHOOD, { nodeId, depth: depth ?? 1 });
}

export async function getProvenanceChain(edgeId: string): Promise<GraphProvenanceEntry[]> {
  return apiClient.request(COMMANDS.graph.GET_PROVENANCE_CHAIN, { edgeId });
}
