import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';
import type { InfrastructureNetworkFact } from '@/types/infrastructure';
import type { InfrastructureHostFact } from '../types';

export interface InfrastructureHostView {
  node: InfrastructureGraphNode;
  fact?: InfrastructureHostFact;
}

export function buildClusterPresentation(
  nodes: InfrastructureGraphNode[],
  edges: InfrastructureGraphEdge[],
  hostFacts: Map<string, InfrastructureHostFact>,
  networkFacts: InfrastructureNetworkFact[] = [],
) {
  const hosts = nodes
    .filter((node) => node.kind === 'physical_host')
    .map((node) => {
      const id = sourceId(node);
      return { node, fact: id ? hostFacts.get(id) : undefined };
    });
  const infrastructure = nodes.filter((node) => ['pve', 'kubernetes', 'ceph', 'ceph_cluster'].includes(node.kind));
  const virtualMachines = nodes.filter((node) => node.kind === 'virtual_machine');
  const operatingSystems = nodes.filter((node) => node.kind === 'os_instance');
  const workloads = nodes.filter((node) => node.domain === 'analysis');
  const storage = nodes.filter((node) => node.domain === 'storage');
  const versionEvidence = hosts
    .map(({ fact }) => formatHostVersion(fact))
    .filter((value): value is string => Boolean(value));
  const kubernetesVersions = networkFacts
    .filter((fact) => fact.factKind === 'kubernetes_version')
    .map((fact) => `${fact.subject}: ${fact.value}`);
  return {
    hosts,
    infrastructure,
    virtualMachines,
    operatingSystems,
    workloads,
    storage,
    relationCount: edges.length,
    versionEvidence: [...new Set([...versionEvidence, ...kubernetesVersions])],
  };
}

export function sourceId(node: InfrastructureGraphNode): string | undefined {
  try {
    const value = JSON.parse(node.provenanceJson) as Record<string, unknown>;
    return typeof value.dataSourceId === 'string' && value.dataSourceId ? value.dataSourceId : undefined;
  } catch {
    return undefined;
  }
}

export function formatHostVersion(fact?: InfrastructureHostFact): string | undefined {
  const value = [fact?.operatingSystem, fact?.operatingSystemVersion, fact?.kernelVersion]
    .filter((part): part is string => Boolean(part))
    .join(' / ');
  return value || undefined;
}
