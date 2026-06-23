import { GraphQuery, GraphQueryResult, GraphSnapshot } from '@/types/models';
import { apiClient } from './client';

export async function getGraphSnapshot(caseId: string): Promise<GraphSnapshot> {
  return apiClient.request('get_graph_snapshot', { request: { caseId } });
}

export async function queryGraph(query: GraphQuery): Promise<GraphQueryResult> {
  return apiClient.request('query_graph', { request: query });
}

export async function getNodeNeighborhood(nodeId: string, depth?: number): Promise<GraphQueryResult> {
  return apiClient.request('get_node_neighborhood', { request: { nodeId, depth: depth ?? 1 } });
}
