import { describe, expect, it } from 'vitest';
import { buildNetworkTopology } from './network-topology';

describe('buildNetworkTopology', () => {
  it('keeps only evidence-backed environment relations', () => {
    const result = buildNetworkTopology({
      nodes: [
        { id: 'pve', domain: 'environment', kind: 'pve', name: 'PVE', status: 'ready', confidence: 'candidate', provenanceJson: '{}' },
        { id: 'host-a', domain: 'environment', kind: 'physical_host', name: 'host-a', status: 'ready', confidence: 'candidate', provenanceJson: '{}' },
        { id: 'host-b', domain: 'environment', kind: 'physical_host', name: 'host-b', status: 'ready', confidence: 'candidate', provenanceJson: '{}' },
        { id: 'artifact', domain: 'analysis', kind: 'audit_log', name: 'audit', status: 'parsed', confidence: 'candidate', provenanceJson: '{}' },
      ],
      edges: [
        { sourceId: 'pve', targetId: 'host-a', relationKind: 'manages', confidence: 'candidate', provenanceJson: '{}' },
        { sourceId: 'pve', targetId: 'host-b', relationKind: 'manages', confidence: 'candidate', provenanceJson: '{}' },
        { sourceId: 'artifact', targetId: 'pve', relationKind: 'derived_from', confidence: 'candidate', provenanceJson: '{}' },
      ],
    });

    expect(result.nodes.map((node) => node.id)).toEqual(['pve', 'host-a', 'host-b']);
    expect(result.edges).toHaveLength(2);
    expect(result.edges.some((edge) => edge.relationKind === 'peer')).toBe(false);
  });

  it('derives host links only from matching network evidence', () => {
    const result = buildNetworkTopology(
      {
        nodes: [
          { id: 'host-a', domain: 'environment', kind: 'physical_host', name: 'host-a', status: 'ready', confidence: 'candidate', provenanceJson: '{}' },
          { id: 'host-b', domain: 'environment', kind: 'physical_host', name: 'host-b', status: 'ready', confidence: 'candidate', provenanceJson: '{}' },
        ],
        edges: [],
      },
      [
        { id: 'link', dataSourceId: 'a', environmentObjectId: 'host-a', fileId: 'f1', sourcePath: '/etc/corosync/corosync.conf', lineNumber: 4, factKind: 'cluster_link', subject: 'host-b', value: 'ring0_addr:192.0.2.20', assertionKind: 'configured', confidence: 'candidate', parser: 'linux.network.config.v1' },
        { id: 'address', dataSourceId: 'b', environmentObjectId: 'host-b', fileId: 'f2', sourcePath: '/etc/network/interfaces', lineNumber: 3, factKind: 'interface_address', subject: 'vmbr0', value: '192.0.2.20/24', assertionKind: 'configured', confidence: 'candidate', parser: 'linux.network.config.v1' },
      ],
    );

    expect(result.edges).toHaveLength(1);
    expect(result.edges[0].relationKind).toBe('network_link');
  });
});
