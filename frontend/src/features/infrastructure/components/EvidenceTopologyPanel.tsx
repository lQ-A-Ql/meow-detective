import { GitBranch } from 'lucide-react';
import type { TFunction } from 'i18next';
import { PanelFrame, SectionHeader } from '@/components/data-display';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { LinuxTopologyEdgeSummary, LinuxTopologyScopeSummary } from '@/types/linuxCluster';
import { statusLabel, statusVariant } from '../model/cluster-status-map';

export function EvidenceTopologyPanel({ scopes, edges, t }: { scopes: LinuxTopologyScopeSummary[]; edges: LinuxTopologyEdgeSummary[]; t: TFunction }) {
  const scopeNames = new Map(scopes.map((scope) => [scope.id, scope.name]));
  return (
    <PanelFrame className="bg-forensics-surface">
      <SectionHeader icon={GitBranch} title={t('infrastructure.workspace.topology.title')} subtitle={t('infrastructure.workspace.topology.description')} />
      <div className="mt-4 grid gap-3 lg:grid-cols-[minmax(0,1fr)_minmax(280px,0.85fr)]">
        <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">{scopes.map((scope) => <article key={scope.id} className="border border-forensics-border p-3"><div className="flex items-start justify-between gap-2"><div className="min-w-0"><div className="truncate text-sm text-forensics-text">{scope.name}</div><div className="mt-1 font-mono text-[10px] text-forensics-muted">{scope.kind} · {scope.memberCount} {t('infrastructure.workspace.topology.members')}</div></div><StatusBadge label={statusLabel(scope.identityState, t)} variant={statusVariant(scope.identityState)} /></div><div className="mt-3 flex items-center gap-2 text-[11px] text-forensics-muted"><span>{t('infrastructure.workspace.topology.completeness')}</span><StatusBadge label={statusLabel(scope.evidenceCompleteness, t)} variant={statusVariant(scope.evidenceCompleteness)} /></div>{scope.diagnostics.length ? <p className="mt-2 line-clamp-2 text-[11px] text-forensics-muted">{scope.diagnostics[0]}</p> : null}</article>)}</div>
        <div className="border border-forensics-border p-3"><div className="text-[10px] uppercase tracking-wide text-forensics-muted">{t('infrastructure.workspace.topology.edges')}</div><div className="mt-2 divide-y divide-forensics-border-light">{edges.map((edge, index) => <div key={`${edge.sourceScopeId}-${edge.targetScopeId}-${edge.kind}-${index}`} className="grid gap-1 py-2 text-[11px]"><div className="flex items-center gap-2 text-forensics-text"><span className="truncate">{scopeNames.get(edge.sourceScopeId) ?? edge.sourceScopeId}</span><span className="text-forensics-muted">→</span><span className="truncate">{scopeNames.get(edge.targetScopeId) ?? edge.targetScopeId}</span></div><div className="flex flex-wrap gap-2 text-[10px] text-forensics-muted"><span>{edge.kind}</span><span>{edge.confidence}</span></div></div>)}{edges.length === 0 ? <div className="py-4 text-xs text-forensics-muted">{t('infrastructure.workspace.topology.noEdges')}</div> : null}</div></div>
      </div>
    </PanelFrame>
  );
}
