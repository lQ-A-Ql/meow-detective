import { describe, expect, it } from 'vitest';
import { buildNetworkTopology } from './network-topology';

describe('buildNetworkTopology', () => {
  it('keeps environment nodes and infers peer host links from shared infrastructure', () => {
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
    expect(result.edges).toEqual(expect.arrayContaining([
      expect.objectContaining({ sourceId: 'host-a', targetId: 'host-b', relationKind: 'peer' }),
    ]));
  });
});
