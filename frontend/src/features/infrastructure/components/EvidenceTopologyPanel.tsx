import { GitBranch } from 'lucide-react';
import type { TFunction } from 'i18next';
import { PanelFrame, SectionHeader } from '@/components/data-display';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { LinuxTopologyEdgeSummary, LinuxTopologyScopeSummary } from '@/types/linuxCluster';
import type { DataSourceSummary } from '@/types/dataSource';
import type { InfrastructureHostFact } from '../types';
import type { InfrastructureNetworkFact } from '@/types/infrastructure';
import { statusLabel, statusVariant } from '../model/cluster-status-map';

export function EvidenceTopologyPanel({ scopes, edges, sourceMetadata, hostFacts, networkFacts, t }: { scopes: LinuxTopologyScopeSummary[]; edges: LinuxTopologyEdgeSummary[]; sourceMetadata: Map<string, DataSourceSummary>; hostFacts: Map<string, InfrastructureHostFact>; networkFacts: InfrastructureNetworkFact[]; t: TFunction }) {
  const scopeNames = new Map(scopes.map((scope) => [scope.id, scope.name]));
  return (
    <PanelFrame className="bg-forensics-surface">
      <SectionHeader icon={GitBranch} title={t('infrastructure.workspace.topology.title')} subtitle={t('infrastructure.workspace.topology.description')} />
      <div className="mt-4 grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(280px,0.85fr)]">
        <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">{scopes.map((scope) => { const details = scopeDetails(scope, sourceMetadata, hostFacts, networkFacts); return <article key={scope.id} className="border border-forensics-border p-3"><div className="flex items-start justify-between gap-2"><div className="min-w-0"><div className="truncate text-sm text-forensics-text">{details.title}</div><div className="mt-1 font-mono text-[10px] text-forensics-muted">{scope.kind} · {scope.memberCount} {t('infrastructure.workspace.topology.members')}</div></div><StatusBadge label={statusLabel(scope.identityState, t)} variant={statusVariant(scope.identityState)} /></div><div className="mt-3 grid gap-1 text-[11px] text-forensics-text-secondary"><div><span className="text-forensics-muted">{t('infrastructure.workspace.topology.hostname')}: </span>{details.hostname ?? '—'}</div><div><span className="text-forensics-muted">{t('infrastructure.workspace.topology.os')}: </span>{details.os ?? '—'}</div><div><span className="text-forensics-muted">{t('infrastructure.workspace.topology.address')}: </span>{details.addresses.length ? details.addresses.join(', ') : '—'}</div><div className="flex flex-wrap items-center gap-1"><span className="text-forensics-muted">{t('infrastructure.workspace.topology.role')}: </span>{details.roles.map((role) => <StatusBadge key={role} label={t(`infrastructure.workspace.topology.roles.${role}`, { defaultValue: role })} variant="outline" />)}</div></div><div className="mt-3 flex items-center gap-2 text-[11px] text-forensics-muted"><span>{t('infrastructure.workspace.topology.completeness')}</span><StatusBadge label={statusLabel(scope.evidenceCompleteness, t)} variant={statusVariant(scope.evidenceCompleteness)} /></div>{scope.diagnostics.length ? <p className="mt-2 line-clamp-2 text-[11px] text-forensics-muted">{scope.diagnostics[0]}</p> : null}</article>; })}</div>
        <div className="border border-forensics-border p-3"><div className="text-[10px] uppercase tracking-wide text-forensics-muted">{t('infrastructure.workspace.topology.edges')}</div><div className="mt-2 divide-y divide-forensics-border-light">{edges.map((edge, index) => <div key={`${edge.sourceScopeId}-${edge.targetScopeId}-${edge.kind}-${index}`} className="grid gap-1 py-2 text-[11px]"><div className="flex items-center gap-2 text-forensics-text"><span className="truncate">{scopeNames.get(edge.sourceScopeId) ?? edge.sourceScopeId}</span><span className="text-forensics-muted">→</span><span className="truncate">{scopeNames.get(edge.targetScopeId) ?? edge.targetScopeId}</span></div><div className="flex flex-wrap gap-2 text-[10px] text-forensics-muted"><span>{t(`infrastructure.workspace.topology.edgeKinds.${edge.kind}`, { defaultValue: edge.kind })}</span><span>{edge.confidence}</span></div></div>)}{edges.length === 0 ? <div className="py-4 text-xs text-forensics-muted">{t('infrastructure.workspace.topology.noEdges')}</div> : null}</div></div>
      </div>
    </PanelFrame>
  );
}

function scopeDetails(scope: LinuxTopologyScopeSummary, sourceMetadata: Map<string, DataSourceSummary>, hostFacts: Map<string, InfrastructureHostFact>, networkFacts: InfrastructureNetworkFact[]) {
  const memberIds = scope.memberSourceIds;
  const facts = networkFacts.filter((fact) => memberIds.includes(fact.dataSourceId));
  const hostFact = memberIds.map((id) => hostFacts.get(id)).find(Boolean);
  const sourceName = memberIds.map((id) => sourceMetadata.get(id)?.name).find(Boolean);
  const addresses = facts.filter((fact) => fact.factKind === 'interface_address').map((fact) => fact.value).filter(Boolean);
  const roles = [...new Set(scope.memberRoles.map((member) => member.role))];
  return {
    title: hostFact?.hostname ?? sourceName ?? scope.name,
    hostname: hostFact?.hostname,
    os: [hostFact?.operatingSystem, hostFact?.operatingSystemVersion, hostFact?.kernelVersion].filter(Boolean).join(' / ') || undefined,
    addresses: [...new Set(addresses)],
    roles: roles.length ? roles : ['unknown'],
  };
}
