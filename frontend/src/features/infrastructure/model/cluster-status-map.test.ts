import { describe, expect, it } from 'vitest';
import { capabilitiesForSummary, provenanceState, trustStages } from './cluster-status-map';
import type { LinuxEvidenceSetSummary } from '@/types/linuxCluster';

const summary = (overrides: Partial<LinuxEvidenceSetSummary> = {}): LinuxEvidenceSetSummary => ({
  importSetId: 'set-1',
  name: 'PVE cluster',
  state: 'ready',
  memberCount: 2,
  readyCount: 2,
  failedCount: 0,
  capabilityLevel: 'metadata_only',
  members: [
    { memberIndex: 0, sourceName: 'node-a', sourcePath: 'node-a.E01', sourceKind: 'e01', importState: 'ready', hashStatus: 'hashed', provenanceStatus: 'complete', addresses: [], roles: [], services: [], containers: [], diagnostics: [] },
    { memberIndex: 1, sourceName: 'node-b', sourcePath: 'node-b.E01', sourceKind: 'e01', importState: 'ready', hashStatus: 'hashed', provenanceStatus: 'complete', addresses: [], roles: [], services: [], containers: [], diagnostics: [] },
  ],
  scopes: [{ id: 'scope:os', kind: 'os', name: 'node-a', identityState: 'candidate', status: 'partial', evidenceCompleteness: 'partial', memberCount: 1, memberSourceIds: ['a'], memberRoles: [{ dataSourceId: 'a', role: 'os_root', memberIndex: 0, confidence: 'candidate' }], diagnostics: [] }],
  edges: [],
  derivedSources: [],
  diagnostics: [],
  ...overrides,
});

describe('cluster status mapping', () => {
  it('keeps partial topology and incomplete provenance visible', () => {
    const value = summary();
    expect(provenanceState(value)).toBe('partial');
    expect(trustStages(value).find((stage) => stage.key === 'topology')?.status).toBe('partial');
  });

  it('does not expose a missing derived source as a preview capability', () => {
    expect(capabilitiesForSummary(summary()).find((item) => item.kind === 'rbd')).toMatchObject({
      level: 'metadata-only',
      status: 'unsupported',
    });
  });

  it('marks failed evidence sets as failed provenance', () => {
    expect(provenanceState(summary({ state: 'failed', failedCount: 1 }))).toBe('failed');
  });
});
