import type { TFunction } from 'i18next';
import type { LinuxEvidenceSetSummary, LinuxTopologyScopeSummary } from '@/types/linuxCluster';

export type EvidenceStatus = 'complete' | 'partial' | 'failed' | 'pending' | 'candidate' | 'proven' | 'indeterminate' | 'unsupported' | 'unknown';

export function evidenceStatus(value: string | undefined): EvidenceStatus {
  const normalized = value?.toLowerCase() ?? 'unknown';
  if (normalized === 'ready' || normalized === 'complete' || normalized === 'hashed' || normalized === 'parsed' || normalized === 'verified') return 'complete';
  if (normalized === 'failed' || normalized === 'conflicted') return 'failed';
  if (normalized === 'pending' || normalized === 'importing') return 'pending';
  if (normalized === 'proven' || normalized === 'corroborated') return 'proven';
  if (normalized === 'candidate' || normalized === 'unproven') return 'candidate';
  if (normalized === 'partial' || normalized === 'ready_metadata' || normalized === 'metadata_only') return 'partial';
  if (normalized === 'indeterminate') return 'indeterminate';
  if (normalized === 'unsupported') return 'unsupported';
  return 'unknown';
}

export function statusLabel(value: string | undefined, t: TFunction): string {
  const key = evidenceStatus(value);
  return t(`infrastructure.workspace.status.${key}`);
}

export function statusVariant(value: string | undefined): 'default' | 'secondary' | 'destructive' | 'outline' {
  switch (evidenceStatus(value)) {
    case 'complete':
    case 'proven':
      return 'default';
    case 'failed':
      return 'destructive';
    case 'partial':
    case 'candidate':
      return 'secondary';
    default:
      return 'outline';
  }
}

export interface ClusterCapability {
  kind: string;
  level: string;
  status: string;
  diagnostics: string[];
}

export function capabilitiesForSummary(summary: LinuxEvidenceSetSummary): ClusterCapability[] {
  const scopes = summary.scopes;
  const scope = (kind: string): LinuxTopologyScopeSummary | undefined => scopes.find((item) => item.kind === kind);
  const host = scope('physical_host') ?? scope('os');
  const ceph = scope('ceph');
  const pve = scope('pve');
  const kubernetes = scope('kubernetes');
  const derived = summary.derivedSources;
  const rbd = derived.find((item) => item.kind.toLowerCase().includes('rbd'));
  const cephFs = derived.find((item) => item.kind.toLowerCase().includes('cephfs'));
  return [
    { kind: 'linux_host', level: host ? 'metadata-browseable' : 'unsupported', status: host?.status ?? 'unsupported', diagnostics: host?.diagnostics ?? [] },
    { kind: 'pve', level: pve ? 'metadata-only' : 'unsupported', status: pve?.identityState ?? 'unsupported', diagnostics: pve?.diagnostics ?? [] },
    { kind: 'ceph', level: ceph ? 'metadata-only' : 'unsupported', status: ceph?.evidenceCompleteness ?? 'unsupported', diagnostics: ceph?.diagnostics ?? [] },
    { kind: 'rbd', level: rbd?.importState === 'ready' ? 'bounded-preview' : 'metadata-only', status: rbd?.provenanceStatus ?? 'unsupported', diagnostics: rbd ? [] : ['没有派生 RBD 数据源'] },
    { kind: 'cephfs', level: cephFs?.importState === 'ready' ? 'bounded-preview' : 'metadata-only', status: cephFs?.provenanceStatus ?? 'indeterminate', diagnostics: cephFs ? [] : ['没有派生 CephFS 数据源'] },
    { kind: 'kubernetes', level: kubernetes ? 'metadata-only' : 'unsupported', status: kubernetes?.identityState ?? 'candidate', diagnostics: kubernetes?.diagnostics ?? [] },
  ];
}

export function provenanceState(summary: LinuxEvidenceSetSummary): 'complete' | 'partial' | 'failed' {
  if (summary.state === 'failed' || summary.failedCount > 0) return 'failed';
  if (!summary.manifestDigest) return 'partial';
  const allHashed = summary.members.length > 0 && summary.members.every((member) => member.hashStatus === 'hashed');
  const completeScopes = summary.scopes.length > 0 && summary.scopes.every((scope) => scope.evidenceCompleteness === 'complete');
  return allHashed && completeScopes ? 'complete' : 'partial';
}

export function trustStages(summary: LinuxEvidenceSetSummary): Array<{ key: string; status: EvidenceStatus }> {
  const provenance = provenanceState(summary);
  return [
    { key: 'manifest', status: summary.diagnostics.some((item) => item.toLowerCase().includes('manifest')) ? 'partial' : 'complete' },
    { key: 'hash', status: summary.members.every((member) => member.hashStatus === 'hashed') ? 'complete' : 'partial' },
    { key: 'sourceDb', status: summary.members.every((member) => member.dataSourceId) ? 'complete' : 'partial' },
    { key: 'topology', status: summary.scopes.length > 0 ? (summary.scopes.every((scope) => scope.evidenceCompleteness === 'complete') ? 'complete' : 'partial') : 'pending' },
    { key: 'lineage', status: summary.derivedSources.length > 0 ? 'partial' : 'indeterminate' },
    { key: 'report', status: provenance },
  ];
}
