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

  // When the backend only exposes a shared parent relation (for example,
  // PVE manages multiple hosts), add deterministic peer links so the canvas
  // reads like a network topology instead of a one-way inventory tree.
  const peerEdges: InfrastructureGraphEdge[] = [];
  for (const parent of nodes) {
    const children = edges
      .filter((edge) => edge.sourceId === parent.id && isHostLike(edge.targetId, nodes))
      .map((edge) => edge.targetId);
    for (let index = 0; index < children.length; index += 1) {
      for (let next = index + 1; next < children.length; next += 1) {
        peerEdges.push({
          sourceId: children[index],
          targetId: children[next],
          relationKind: 'peer',
          confidence: 'candidate',
          provenanceJson: JSON.stringify({ basis: 'shared-parent', parentId: parent.id }),
        });
      }
    }
  }

  return { nodes, edges: [...edges, ...peerEdges] };
}

function isHostLike(nodeId: string, nodes: InfrastructureGraphNode[]) {
  return nodes.find((node) => node.id === nodeId)?.kind === 'physical_host';
}
