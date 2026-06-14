import { useQuery } from '@tanstack/react-query';
import { getGraphSnapshot, getNodeNeighborhood, queryGraph } from '@/lib/api/graph';
import { GraphQuery } from '@/types/models';

export function useGraphSnapshot(caseId: string) {
  return useQuery({
    queryKey: ['graph', 'snapshot', caseId],
    queryFn: () => getGraphSnapshot(caseId),
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
