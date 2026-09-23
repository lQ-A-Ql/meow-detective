import { useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useTranslation } from 'react-i18next';
import { useCurrentCase } from '@/features/case/hooks';
import { getInfrastructureGraph } from '@/lib/api/infrastructure';
import { groupNodesByDomain, matchesInfrastructureQuery } from '../logic/labels';
import { buildNetworkTopology } from '../logic/network-topology';
import { useInfrastructureHostFacts } from './useInfrastructureHostFacts';

export function useInfrastructureWorkspace() {
  const { t } = useTranslation();
  const currentCase = useCurrentCase();
  const hostFacts = useInfrastructureHostFacts();
  const graph = useQuery({
    queryKey: ['infrastructure', 'graph', currentCase.data?.id ?? null],
    queryFn: getInfrastructureGraph,
    enabled: Boolean(currentCase.data),
  });
  const [selectedId, setSelectedId] = useState<string>();
  const [query, setQuery] = useState('');
  const nodes = graph.data?.nodes ?? [];
  const edges = graph.data?.edges ?? [];
  const selected = nodes.find((node) => node.id === selectedId);
  const domains = useMemo(() => groupNodesByDomain(nodes), [nodes]);
  const visibleNodes = useMemo(
    () => nodes.filter((node) => matchesInfrastructureQuery(node, query, t)),
    [nodes, query, t],
  );
  const nodeNames = useMemo(() => new Map(nodes.map((node) => [node.id, node.name])), [nodes]);
  const networkFacts = graph.data?.networkFacts ?? [];
  const networkTopology = useMemo(() => buildNetworkTopology({ nodes, edges }, networkFacts), [edges, networkFacts, nodes]);

  return {
    currentCase,
    graph,
    nodes,
    edges,
    domains,
    visibleNodes,
    nodeNames,
    networkTopology,
    networkFacts,
    hostFacts,
    query,
    selected,
    selectedId,
    setQuery,
    setSelectedId,
  };
}
