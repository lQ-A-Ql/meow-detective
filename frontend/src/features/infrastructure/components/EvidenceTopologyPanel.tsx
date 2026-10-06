import { GitBranch } from 'lucide-react';
import type { TFunction } from 'i18next';
import { useMemo } from 'react';
import { PanelFrame, SectionHeader } from '@/components/data-display';
import { VirtualGrid } from '@/components/lists/VirtualGrid';
import { VirtualList } from '@/components/lists/VirtualList';
import { keyedItems } from '@/lib/keyed-items';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { DataSourceSummary } from '@/types/dataSource';
import type { InfrastructureNetworkFact } from '@/types/infrastructure';
import type { InfrastructureHostFact } from '../types';
import type { LinuxTopologyEdgeSummary, LinuxTopologyScopeSummary } from '@/types/linuxCluster';
import type { LinuxEvidenceSetMemberSummary } from '@/types/linuxCluster';
import { statusLabel, statusVariant } from '../model/cluster-status-map';

interface EvidenceTopologyPanelProps {
  scopes: LinuxTopologyScopeSummary[];
  edges: LinuxTopologyEdgeSummary[];
  sourceMetadata: Map<string, DataSourceSummary>;
  hostFacts: Map<string, InfrastructureHostFact>;
  networkFacts: InfrastructureNetworkFact[];
  members: LinuxEvidenceSetMemberSummary[];
  t: TFunction;
}

/** One card per evidence source. Scopes are layers inside the source card. */
export function EvidenceTopologyPanel({ scopes, edges, sourceMetadata, hostFacts, networkFacts, members, t }: EvidenceTopologyPanelProps) {
  const scopeNames = useMemo(() => new Map(scopes.map((scope) => [scope.id, scope.name])), [scopes]);
  const groups = useMemo(() => {
    const sourceIds = [...new Set(scopes.flatMap((scope) => scope.memberSourceIds))];
    const memberMap = new Map(members.map((member) => [member.dataSourceId, member]));
    return sourceIds.map((sourceId) => sourceGroup(sourceId, scopes, sourceMetadata, hostFacts, networkFacts, memberMap.get(sourceId)));
  }, [hostFacts, members, networkFacts, scopes, sourceMetadata]);
  const topologyEdges = useMemo(() => keyedItems(edges, getTopologyEdgeIdentity), [edges]);
  return (
    <PanelFrame className="bg-forensics-surface">
      <SectionHeader icon={GitBranch} title={t('infrastructure.workspace.topology.title')} subtitle={t('infrastructure.workspace.topology.description')} />
      <div className="mt-4 grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(280px,0.85fr)]">
        <div>
          {groups.length > 0 ? <VirtualGrid items={groups} getItemKey={(group) => group.sourceId}
            resetKey={groups.map((group) => group.sourceId).join('|')}
            renderItem={(group) => <SourceTopologyCard group={group} t={t} />} /> :
            <div className="border border-dashed border-forensics-border-strong p-4 text-xs text-forensics-muted">{t('infrastructure.workspace.topology.noSources')}</div>}
        </div>
        <div className="border border-forensics-border p-3">
          <div className="text-[10px] uppercase tracking-wide text-forensics-muted">{t('infrastructure.workspace.topology.edges')}</div>
          {topologyEdges.length > 0 ? <VirtualList items={topologyEdges} getItemKey={(row) => row.key} estimateSize={62}
            style={{ height: 'min(60vh, 540px)' }} viewportClassName="mt-2"
            renderItem={({ item: edge }) => <div className="grid gap-1 border-b border-forensics-border-light py-2 text-[11px]"><div className="flex items-center gap-2 text-forensics-text"><span className="truncate">{scopeNames.get(edge.sourceScopeId) ?? edge.sourceScopeId}</span><span className="text-forensics-muted">→</span><span className="truncate">{scopeNames.get(edge.targetScopeId) ?? edge.targetScopeId}</span></div><div className="flex flex-wrap gap-2 text-[10px] text-forensics-muted"><span>{t(`infrastructure.workspace.topology.edgeKinds.${edge.kind}`, { defaultValue: edge.kind })}</span><span>{edge.confidence}</span></div></div>} /> :
            <div className="py-4 text-xs text-forensics-muted">{t('infrastructure.workspace.topology.noEdges')}</div>}
        </div>
      </div>
    </PanelFrame>
  );
}

const getTopologyEdgeIdentity = (edge: LinuxTopologyEdgeSummary) => JSON.stringify([
  edge.sourceScopeId,
  edge.targetScopeId,
  edge.kind,
  edge.confidence,
]);

interface SourceTopologyGroup {
  sourceId: string;
  source?: DataSourceSummary;
  hostname?: string;
  os?: string;
  addresses: string[];
  roles: string[];
  scopes: LinuxTopologyScopeSummary[];
}

function sourceGroup(sourceId: string, scopes: LinuxTopologyScopeSummary[], sourceMetadata: Map<string, DataSourceSummary>, hostFacts: Map<string, InfrastructureHostFact>, networkFacts: InfrastructureNetworkFact[], member?: LinuxEvidenceSetMemberSummary): SourceTopologyGroup {
  const sourceScopes = scopes.filter((scope) => scope.memberSourceIds.includes(sourceId));
  const hostFact = hostFacts.get(sourceId);
  const addresses = [...new Set(networkFacts.filter((fact) => fact.dataSourceId === sourceId && fact.factKind === 'interface_address').map((fact) => fact.value).filter(Boolean))];
  const roles = [...new Set(sourceScopes.flatMap((scope) => scope.memberRoles.filter((member) => member.dataSourceId === sourceId).map((member) => member.role)))];
  return {
    sourceId,
    source: sourceMetadata.get(sourceId),
    hostname: member?.hostname ?? hostFact?.hostname,
    os: member ? [member.operatingSystem, member.osVersion, member.kernelVersion].filter(Boolean).join(' / ') || undefined : [hostFact?.operatingSystem, hostFact?.operatingSystemVersion, hostFact?.kernelVersion].filter(Boolean).join(' / ') || undefined,
    addresses: member?.addresses.length ? member.addresses : addresses,
    roles: member?.roles.length ? member.roles : roles.length ? roles : ['unknown'],
    scopes: sourceScopes,
  };
}

function SourceTopologyCard({ group, t }: { group: SourceTopologyGroup; t: TFunction }) {
  const sourceName = group.source?.name ?? group.sourceId;
  return <article className="border border-forensics-border p-4"><div className="flex items-start justify-between gap-3"><div className="min-w-0"><div className="truncate text-sm text-forensics-text" title={sourceName}>{sourceName}</div><div className="mt-1 truncate font-mono text-[10px] text-forensics-muted" title={group.source?.sourcePath}>{group.source?.kind?.toUpperCase() ?? 'SOURCE'} · {group.sourceId}</div></div><StatusBadge label={t('infrastructure.workspace.topology.source')} variant="outline" /></div><div className="mt-3 grid gap-1 text-[11px] text-forensics-text-secondary"><div><span className="text-forensics-muted">{t('infrastructure.workspace.topology.hostname')}: </span>{group.hostname ?? '—'}</div><div><span className="text-forensics-muted">{t('infrastructure.workspace.topology.os')}: </span>{group.os ?? '—'}</div><div><span className="text-forensics-muted">{t('infrastructure.workspace.topology.address')}: </span>{group.addresses.length ? group.addresses.join(', ') : '—'}</div><div className="flex flex-wrap items-center gap-1"><span className="text-forensics-muted">{t('infrastructure.workspace.topology.role')}: </span>{group.roles.map((role) => <StatusBadge key={role} label={t(`infrastructure.workspace.topology.roles.${role}`, { defaultValue: role })} variant="outline" />)}</div></div><div className="mt-4 border-t border-forensics-border-light pt-3"><div className="text-[10px] uppercase tracking-wide text-forensics-muted">{t('infrastructure.workspace.topology.layers')}</div><div className="mt-2 space-y-1">{group.scopes.map((scope) => <div key={scope.id} className="flex items-center justify-between gap-2 text-[11px]"><span className="truncate text-forensics-text">{t(`infrastructure.kinds.${scope.kind}`, { defaultValue: scope.name })}</span><StatusBadge label={statusLabel(scope.evidenceCompleteness, t)} variant={statusVariant(scope.evidenceCompleteness)} /></div>)}</div></div></article>;
}
