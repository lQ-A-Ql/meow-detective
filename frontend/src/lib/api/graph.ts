import {
  GraphNode,
  GraphProvenanceEntry,
  GraphQuery,
  GraphQueryResult,
  GraphSnapshot,
} from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export async function getGraphSnapshot(): Promise<GraphSnapshot> {
  return apiClient.request(COMMANDS.graph.GET_GRAPH_SNAPSHOT, {});
}

export async function queryGraph(query: GraphQuery): Promise<GraphQueryResult> {
  return apiClient.request(COMMANDS.graph.QUERY_GRAPH, { query });
}

export async function listGraphNodes(limit = 100, offset = 0): Promise<GraphNode[]> {
  return apiClient.request(COMMANDS.graph.LIST_GRAPH_NODES, { request: { limit, offset } });
}

export async function getNodeNeighborhood(nodeId: string, depth?: number): Promise<GraphQueryResult> {
  // The command takes a single `request` argument (GetNodeNeighborhoodRequest); passing the
  // DTO fields flat made Tauri reject the call with a missing `request` argument.
  return apiClient.request(COMMANDS.graph.GET_NODE_NEIGHBORHOOD, { request: { nodeId, depth: depth ?? 1 } });
}

export async function getProvenanceChain(edgeId: string): Promise<GraphProvenanceEntry[]> {
  // Same wrapper contract as get_node_neighborhood (GetProvenanceChainRequest).
  return apiClient.request(COMMANDS.graph.GET_PROVENANCE_CHAIN, { request: { edgeId } });
}
