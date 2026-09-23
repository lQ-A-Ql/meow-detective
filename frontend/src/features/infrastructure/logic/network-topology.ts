import type { InfrastructureGraph, InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';
import type { InfrastructureNetworkFact } from '@/types/infrastructure';

export interface NetworkTopology {
  nodes: InfrastructureGraphNode[];
  edges: InfrastructureGraphEdge[];
}

const NETWORK_NODE_KINDS = new Set([
  'physical_host',
  'pve',
  'ceph',
  'ceph_cluster',
  'virtual_machine',
  'os_instance',
  'kubernetes',
]);

/** Projects the evidence graph into an environment/network view. */
export function buildNetworkTopology(graph: InfrastructureGraph, networkFacts: InfrastructureNetworkFact[] = []): NetworkTopology {
  const nodes = graph.nodes.filter((node) => NETWORK_NODE_KINDS.has(node.kind));
  const nodeIds = new Set(nodes.map((node) => node.id));
  const edges = graph.edges.filter((edge) => nodeIds.has(edge.sourceId) && nodeIds.has(edge.targetId));

  return { nodes, edges: [...edges, ...deriveNetworkLinks(nodes, networkFacts)] };
}

function deriveNetworkLinks(nodes: InfrastructureGraphNode[], facts: InfrastructureNetworkFact[]): InfrastructureGraphEdge[] {
  const nodeIds = new Set(nodes.map((node) => node.id));
  const addressOwners = new Map<string, { nodeId: string; factId: string }>();
  const ambiguousAddresses = new Set<string>();
  for (const fact of facts) {
    if (!nodeIds.has(fact.environmentObjectId) || !['interface_address', 'hostname_mapping'].includes(fact.factKind)) continue;
    const address = normalizeAddress(fact.value);
    if (!address || ambiguousAddresses.has(address)) continue;
    const previous = addressOwners.get(address);
    if (previous && previous.nodeId !== fact.environmentObjectId) {
      addressOwners.delete(address);
      ambiguousAddresses.add(address);
    } else {
      addressOwners.set(address, { nodeId: fact.environmentObjectId, factId: fact.id });
    }
  }
  const links = new Map<string, InfrastructureGraphEdge>();
  for (const fact of facts) {
    if (fact.factKind !== 'cluster_link' || !nodeIds.has(fact.environmentObjectId)) continue;
    const address = normalizeAddress(fact.value.split(':').pop() ?? '');
    const target = address ? addressOwners.get(address) : undefined;
    if (!target || target.nodeId === fact.environmentObjectId) continue;
    const [sourceId, targetId] = [fact.environmentObjectId, target.nodeId].sort();
    const key = `${sourceId}:${targetId}:${address}`;
    links.set(key, {
      sourceId,
      targetId,
      relationKind: 'network_link',
      confidence: fact.confidence,
      provenanceJson: JSON.stringify({ basis: 'network_facts', sourceFactId: fact.id, targetFactId: target.factId }),
    });
  }
  return [...links.values()];
}

function normalizeAddress(value: string): string | undefined {
  const candidate = value.trim().split('/')[0];
  return /^\d{1,3}(?:\.\d{1,3}){3}$/.test(candidate) || candidate.includes(':') ? candidate : undefined;
}
