import { describe, expect, it } from 'vitest';
import { buildClusterPresentation, formatHostVersion } from './cluster-presentation';

describe('buildClusterPresentation', () => {
  it('separates hosts, infrastructure, workloads and storage without treating evidence as runtime state', () => {
    const result = buildClusterPresentation([
      { id: 'host', domain: 'environment', kind: 'physical_host', name: 'host-1', status: 'ready', confidence: 'candidate', provenanceJson: '{"dataSourceId":"source-1"}' },
      { id: 'k8s', domain: 'environment', kind: 'kubernetes', name: 'Kubernetes', status: 'partial', confidence: 'candidate', provenanceJson: '{}' },
      { id: 'artifact', domain: 'analysis', kind: 'container_log', name: 'container log', status: 'parsed', confidence: 'candidate', provenanceJson: '{}' },
      { id: 'ceph', domain: 'storage', kind: 'ceph_cluster', name: 'Ceph', status: 'ready', confidence: 'candidate', provenanceJson: '{}' },
    ], [], new Map([['source-1', { hostname: 'host-1', operatingSystem: 'openEuler', operatingSystemVersion: '24.03', kernelVersion: '6.6' }]]), [
      { id: 'version', dataSourceId: 'source-1', environmentObjectId: 'host', fileId: 'manifest', sourcePath: '/etc/kubernetes/manifests/kube-apiserver.yaml', lineNumber: 1, factKind: 'kubernetes_version', subject: 'kube-apiserver', value: 'v1.30.1', assertionKind: 'configured', confidence: 'candidate', parser: 'kubernetes.network.resources.v1' },
    ]);

    expect(result.hosts).toHaveLength(1);
    expect(result.infrastructure.map((node) => node.id)).toEqual(['k8s', 'ceph']);
    expect(result.workloads.map((node) => node.id)).toEqual(['artifact']);
    expect(result.storage.map((node) => node.id)).toEqual(['ceph']);
    expect(result.versionEvidence).toEqual(['openEuler / 24.03 / 6.6', 'kube-apiserver: v1.30.1']);
  });

  it('does not fabricate a host version when no parsed system evidence exists', () => {
    expect(formatHostVersion()).toBeUndefined();
  });
});
