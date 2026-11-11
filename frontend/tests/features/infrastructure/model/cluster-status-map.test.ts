import { describe, expect, it } from 'vitest';
import type { TFunction } from 'i18next';
import i18n from '@/i18n';
import {
  capabilitiesForSummary,
  evidenceStatus,
  provenanceState,
  statusLabel,
  statusVariant,
  trustStages,
  type EvidenceStatus,
} from '@/features/infrastructure/model/cluster-status-map';
import type {
  LinuxEvidenceSetMemberSummary,
  LinuxEvidenceSetSummary,
  LinuxTopologyScopeSummary,
} from '@/types/linuxCluster';

const echo = ((key: string) => key) as unknown as TFunction;

const member = (overrides: Partial<LinuxEvidenceSetMemberSummary> = {}): LinuxEvidenceSetMemberSummary => ({
  memberIndex: 0,
  sourceName: 'node-a',
  sourcePath: 'node-a.E01',
  sourceKind: 'e01',
  importState: 'ready',
  hashStatus: 'hashed',
  provenanceStatus: 'complete',
  addresses: [],
  roles: [],
  services: [],
  containers: [],
  diagnostics: [],
  ...overrides,
});

const scope = (overrides: Partial<LinuxTopologyScopeSummary> = {}): LinuxTopologyScopeSummary => ({
  id: 'scope:os',
  kind: 'os',
  name: 'node-a',
  identityState: 'candidate',
  status: 'partial',
  evidenceCompleteness: 'partial',
  memberCount: 1,
  memberSourceIds: ['a'],
  memberRoles: [],
  diagnostics: [],
  ...overrides,
});

const summary = (overrides: Partial<LinuxEvidenceSetSummary> = {}): LinuxEvidenceSetSummary => ({
  importSetId: 'set-1',
  name: 'PVE cluster',
  state: 'ready',
  memberCount: 2,
  readyCount: 2,
  failedCount: 0,
  capabilityLevel: 'metadata_only',
  members: [
    {
      memberIndex: 0,
      sourceName: 'node-a',
      sourcePath: 'node-a.E01',
      sourceKind: 'e01',
      importState: 'ready',
      hashStatus: 'hashed',
      provenanceStatus: 'complete',
      addresses: [],
      roles: [],
      services: [],
      containers: [],
      diagnostics: [],
    },
    {
      memberIndex: 1,
      sourceName: 'node-b',
      sourcePath: 'node-b.E01',
      sourceKind: 'e01',
      importState: 'ready',
      hashStatus: 'hashed',
      provenanceStatus: 'complete',
      addresses: [],
      roles: [],
      services: [],
      containers: [],
      diagnostics: [],
    },
  ],
  scopes: [
    {
      id: 'scope:os',
      kind: 'os',
      name: 'node-a',
      identityState: 'candidate',
      status: 'partial',
      evidenceCompleteness: 'partial',
      memberCount: 1,
      memberSourceIds: ['a'],
      memberRoles: [{ dataSourceId: 'a', role: 'os_root', memberIndex: 0, confidence: 'candidate' }],
      diagnostics: [],
    },
  ],
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

describe('evidenceStatus', () => {
  const cases: Array<[string | undefined, EvidenceStatus]> = [
    [undefined, 'unknown'],
    ['', 'unknown'],
    ['READY', 'complete'],
    ['hashed', 'complete'],
    ['parsed', 'complete'],
    ['verified', 'complete'],
    ['recorded', 'complete'],
    ['complete', 'complete'],
    ['failed', 'failed'],
    ['conflicted', 'failed'],
    ['pending', 'pending'],
    ['importing', 'pending'],
    ['proven', 'proven'],
    ['corroborated', 'proven'],
    ['candidate', 'candidate'],
    ['unproven', 'candidate'],
    ['partial', 'partial'],
    ['ready_metadata', 'partial'],
    ['metadata_only', 'partial'],
    ['indeterminate', 'indeterminate'],
    ['unsupported', 'unsupported'],
    ['mystery-state', 'unknown'],
  ];

  it.each(cases)('normalizes %s to %s', (value, expected) => {
    expect(evidenceStatus(value)).toBe(expected);
  });

  it('labels the normalized status through its dedicated i18n key', () => {
    expect(statusLabel('hashed', echo)).toBe('infrastructure.workspace.status.complete');
    expect(statusLabel(undefined, echo)).toBe('infrastructure.workspace.status.unknown');
    expect(statusLabel('failed', echo)).toBe('infrastructure.workspace.status.failed');
  });

  it.each([
    ['ready', 'default'],
    ['proven', 'default'],
    ['failed', 'destructive'],
    ['partial', 'secondary'],
    ['candidate', 'secondary'],
    ['pending', 'outline'],
    ['unsupported', 'outline'],
    ['indeterminate', 'outline'],
    [undefined, 'outline'],
  ] as const)('renders %s with the %s badge variant', (value, expected) => {
    expect(statusVariant(value)).toBe(expected);
  });
});

describe('capabilitiesForSummary', () => {
  it('derives levels and statuses from scopes and ready derived sources', () => {
    const capabilities = capabilitiesForSummary(
      summary({
        scopes: [
          scope({
            id: 'scope:host',
            kind: 'physical_host',
            status: 'ready',
            identityState: 'verified',
            evidenceCompleteness: 'complete',
            diagnostics: ['host diagnostic'],
          }),
          scope({
            id: 'scope:pve',
            kind: 'pve',
            status: 'partial',
            identityState: 'proven',
            evidenceCompleteness: 'complete',
          }),
          scope({ id: 'scope:ceph', kind: 'ceph', status: 'partial', evidenceCompleteness: 'partial' }),
          scope({
            id: 'scope:k8s',
            kind: 'kubernetes',
            status: 'partial',
            identityState: 'candidate',
            evidenceCompleteness: 'partial',
            diagnostics: ['k8s note'],
          }),
        ],
        derivedSources: [
          {
            dataSourceId: 'rbd-1',
            kind: 'CEPH_RBD',
            importState: 'ready',
            provenanceStatus: 'complete',
            sourcePath: '/dev/rbd0',
          },
          {
            dataSourceId: 'fs-1',
            kind: 'cephfs',
            importState: 'ready',
            provenanceStatus: 'candidate',
            sourcePath: '/mnt/cephfs',
          },
        ],
      }),
    );

    expect(capabilities.map((item) => item.kind)).toEqual(['linux_host', 'pve', 'ceph', 'rbd', 'cephfs', 'kubernetes']);
    expect(capabilities.find((item) => item.kind === 'linux_host')).toEqual({
      kind: 'linux_host',
      level: 'metadata-browseable',
      status: 'ready',
      diagnostics: ['host diagnostic'],
    });
    expect(capabilities.find((item) => item.kind === 'pve')).toMatchObject({
      level: 'metadata-only',
      status: 'proven',
    });
    expect(capabilities.find((item) => item.kind === 'ceph')).toMatchObject({
      level: 'metadata-only',
      status: 'partial',
    });
    expect(capabilities.find((item) => item.kind === 'rbd')).toEqual({
      kind: 'rbd',
      level: 'bounded-preview',
      status: 'complete',
      diagnostics: [],
    });
    expect(capabilities.find((item) => item.kind === 'cephfs')).toEqual({
      kind: 'cephfs',
      level: 'bounded-preview',
      status: 'candidate',
      diagnostics: [],
    });
    expect(capabilities.find((item) => item.kind === 'kubernetes')).toMatchObject({
      level: 'metadata-only',
      status: 'candidate',
      diagnostics: ['k8s note'],
    });
  });

  it('treats an os scope as the host fallback when no physical_host scope exists', () => {
    const host = capabilitiesForSummary(
      summary({
        scopes: [scope({ kind: 'os', status: 'ready', identityState: 'verified' })],
      }),
    ).find((item) => item.kind === 'linux_host');

    expect(host).toMatchObject({ level: 'metadata-browseable', status: 'ready' });
  });

  it('reports the host capability unsupported when no host or os scope exists', () => {
    const host = capabilitiesForSummary(summary({ scopes: [] })).find((item) => item.kind === 'linux_host');

    expect(host).toEqual({
      kind: 'linux_host',
      level: 'unsupported',
      status: 'unsupported',
      diagnostics: [i18n.t('infrastructure.capabilitiesDiagnostics.linuxHost')],
    });
  });

  it('reports unsupported defaults with explanatory diagnostics for missing evidence', () => {
    const capabilities = capabilitiesForSummary(summary());

    expect(capabilities.find((item) => item.kind === 'pve')).toEqual({
      kind: 'pve',
      level: 'unsupported',
      status: 'unsupported',
      diagnostics: [i18n.t('infrastructure.capabilitiesDiagnostics.pve')],
    });
    expect(capabilities.find((item) => item.kind === 'ceph')).toEqual({
      kind: 'ceph',
      level: 'unsupported',
      status: 'unsupported',
      diagnostics: [i18n.t('infrastructure.capabilitiesDiagnostics.ceph')],
    });
    expect(capabilities.find((item) => item.kind === 'rbd')).toEqual({
      kind: 'rbd',
      level: 'metadata-only',
      status: 'unsupported',
      diagnostics: [i18n.t('infrastructure.capabilitiesDiagnostics.rbd')],
    });
    expect(capabilities.find((item) => item.kind === 'cephfs')).toEqual({
      kind: 'cephfs',
      level: 'metadata-only',
      status: 'indeterminate',
      diagnostics: [i18n.t('infrastructure.capabilitiesDiagnostics.cephfs')],
    });
    expect(capabilities.find((item) => item.kind === 'kubernetes')).toEqual({
      kind: 'kubernetes',
      level: 'unsupported',
      status: 'candidate',
      diagnostics: [i18n.t('infrastructure.capabilitiesDiagnostics.kubernetes')],
    });
  });

  it('keeps a derived source visible as metadata-only when it is not import-ready', () => {
    const capabilities = capabilitiesForSummary(
      summary({
        derivedSources: [
          {
            dataSourceId: 'rbd-1',
            kind: 'ceph_rbd',
            importState: 'importing',
            provenanceStatus: 'partial',
            sourcePath: '/dev/rbd0',
          },
        ],
      }),
    );

    expect(capabilities.find((item) => item.kind === 'rbd')).toEqual({
      kind: 'rbd',
      level: 'metadata-only',
      status: 'partial',
      diagnostics: [],
    });
  });
});

describe('provenanceState', () => {
  it('is complete only with a manifest digest, hashed members and complete scopes', () => {
    const complete = summary({
      manifestDigest: 'sha256:abc',
      members: [member({ dataSourceId: 'a' }), member({ memberIndex: 1, dataSourceId: 'b' })],
      scopes: [scope({ evidenceCompleteness: 'complete' })],
    });

    expect(provenanceState(complete)).toBe('complete');
  });

  it('treats a missing manifest digest as partial even when every scope is complete', () => {
    expect(provenanceState(summary({ scopes: [scope({ evidenceCompleteness: 'complete' })] }))).toBe('partial');
  });

  it('treats an empty member or scope inventory as partial', () => {
    expect(provenanceState(summary({ manifestDigest: 'sha256:abc', members: [] }))).toBe('partial');
    expect(provenanceState(summary({ manifestDigest: 'sha256:abc', scopes: [] }))).toBe('partial');
  });

  it('treats a pending member hash or a failed member count as incomplete', () => {
    expect(
      provenanceState(
        summary({
          manifestDigest: 'sha256:abc',
          members: [member({ hashStatus: 'pending' })],
          scopes: [scope({ evidenceCompleteness: 'complete' })],
        }),
      ),
    ).toBe('partial');
    expect(provenanceState(summary({ failedCount: 1 }))).toBe('failed');
  });
});

describe('trustStages', () => {
  it('marks every stage complete for a clean, fully hashed evidence set', () => {
    const stages = trustStages(
      summary({
        manifestDigest: 'sha256:abc',
        members: [member({ dataSourceId: 'a' }), member({ memberIndex: 1, dataSourceId: 'b' })],
        scopes: [scope({ evidenceCompleteness: 'complete' })],
        derivedSources: [],
      }),
    );

    expect(stages).toEqual([
      { key: 'manifest', status: 'complete' },
      { key: 'hash', status: 'complete' },
      { key: 'sourceDb', status: 'complete' },
      { key: 'topology', status: 'complete' },
      { key: 'lineage', status: 'indeterminate' },
      { key: 'report', status: 'complete' },
    ]);
  });

  it('degrades the manifest stage when diagnostics mention a manifest problem', () => {
    const stages = trustStages(summary({ diagnostics: ['MANIFEST digest is absent'] }));

    expect(stages.find((stage) => stage.key === 'manifest')?.status).toBe('partial');
  });

  it('degrades source database and hashing stages when member evidence is untraceable', () => {
    const stages = trustStages(summary({ members: [member({ dataSourceId: null, hashStatus: 'pending' })] }));

    expect(stages.find((stage) => stage.key === 'hash')?.status).toBe('partial');
    expect(stages.find((stage) => stage.key === 'sourceDb')?.status).toBe('partial');
  });

  it('keeps topology pending without scopes and lineage partial with derived sources', () => {
    const stages = trustStages(
      summary({
        scopes: [],
        derivedSources: [
          {
            dataSourceId: 'rbd-1',
            kind: 'ceph_rbd',
            importState: 'ready',
            provenanceStatus: 'complete',
            sourcePath: '/dev/rbd0',
          },
        ],
      }),
    );

    expect(stages.find((stage) => stage.key === 'topology')?.status).toBe('pending');
    expect(stages.find((stage) => stage.key === 'lineage')?.status).toBe('partial');
  });

  it('mirrors the report gate to the overall provenance state', () => {
    expect(trustStages(summary()).find((stage) => stage.key === 'report')?.status).toBe('partial');
    expect(
      trustStages(
        summary({
          manifestDigest: 'sha256:abc',
          members: [member({ dataSourceId: 'a' })],
          scopes: [scope({ evidenceCompleteness: 'complete' })],
        }),
      ).find((stage) => stage.key === 'report')?.status,
    ).toBe('complete');
  });
});
