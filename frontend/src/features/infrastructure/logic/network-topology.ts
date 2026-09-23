import type { InfrastructureGraph, InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';

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
export function buildNetworkTopology(graph: InfrastructureGraph): NetworkTopology {
  const nodes = graph.nodes.filter((node) => NETWORK_NODE_KINDS.has(node.kind));
  const nodeIds = new Set(nodes.map((node) => node.id));
  const edges = graph.edges.filter((edge) => nodeIds.has(edge.sourceId) && nodeIds.has(edge.targetId));

  return { nodes, edges };
}
