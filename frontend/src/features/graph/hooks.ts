import { useQuery } from '@tanstack/react-query';
import {
  getGraphSnapshot,
  listGraphNodes,
  getNodeNeighborhood,
  getProvenanceChain,
  queryGraph,
} from '@/lib/api/graph';
import { GraphNode, GraphQuery } from '@/types/models';

export function useGraphSnapshot(caseId: string) {
  return useQuery({
    queryKey: ['graph', 'snapshot', caseId],
    queryFn: () => getGraphSnapshot(),
    enabled: Boolean(caseId),
    retry: false,
  });
}

export function useGraphQuery(query: GraphQuery) {
  return useQuery({
    queryKey: ['graph', 'query', query],
    queryFn: () => queryGraph(query),
    enabled: query.startIds.length > 0,
    retry: false,
  });
}

export function useNodeNeighborhood(nodeId: string, depth: number = 1) {
  return useQuery({
    queryKey: ['graph', 'neighborhood', nodeId, depth],
    queryFn: () => getNodeNeighborhood(nodeId, depth),
    enabled: Boolean(nodeId),
    retry: false,
  });
}

export function useGraphNodes(caseId: string, limit = 100, offset = 0) {
  return useQuery({
    queryKey: ['graph', 'nodes', caseId, limit, offset],
    queryFn: () => listGraphNodes(limit, offset),
    enabled: Boolean(caseId),
    retry: false,
  });
}

export function useGraphCitationNodes(caseId: string, seedNodeIds: string[]) {
  return useQuery({
    queryKey: ['graph', 'citation-nodes', caseId, seedNodeIds],
    queryFn: async () => {
      const nodes: GraphNode[] = [];
      const seen = new Set<string>();

      for (const nodeId of seedNodeIds) {
        const result = await getNodeNeighborhood(nodeId, 1);
        for (const node of result.nodes) {
          if (!seen.has(node.id)) {
            seen.add(node.id);
            nodes.push(node);
          }
        }
      }

      return nodes;
    },
    enabled: Boolean(caseId) && seedNodeIds.length > 0,
    retry: false,
  });
}

export function useProvenanceChain(edgeId?: string) {
  return useQuery({
    queryKey: ['graph', 'provenance', edgeId ?? ''],
    queryFn: () => getProvenanceChain(edgeId!),
    enabled: Boolean(edgeId),
    retry: false,
  });
}
