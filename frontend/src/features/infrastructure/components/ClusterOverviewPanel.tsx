import { CircleDot, Server } from 'lucide-react';
import type { TFunction } from 'i18next';
import { KeyValueField, PanelFrame, SectionHeader } from '@/components/data-display';
import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';
import type { InfrastructureNetworkFact } from '@/types/infrastructure';
import type { InfrastructureHostFact } from '../types';
import { buildClusterPresentation, formatHostVersion } from '../logic/cluster-presentation';
import { kindLabel, stateLabel } from '../logic/labels';

export function ClusterOverviewPanel({
  nodes,
  edges,
  hostFacts,
  networkFacts,
  t,
}: {
  nodes: InfrastructureGraphNode[];
  edges: InfrastructureGraphEdge[];
  hostFacts: Map<string, InfrastructureHostFact>;
  networkFacts: InfrastructureNetworkFact[];
  t: TFunction;
}) {
  const presentation = buildClusterPresentation(nodes, edges, hostFacts, networkFacts);
  const platforms = presentation.infrastructure.map((node) => kindLabel(node.kind, t));
  return (
    <div className="space-y-4">
      <PanelFrame className="bg-forensics-surface">
        <SectionHeader icon={CircleDot} title={t('infrastructure.clusterInfo.title')} subtitle={t('infrastructure.clusterInfo.description')} />
        <div className="mt-4 grid gap-3 text-xs sm:grid-cols-4">
          <KeyValueField layout="stacked" label={t('infrastructure.clusterInfo.types')} value={platforms.join(' / ') || t('infrastructure.values.unavailable')} />
          <KeyValueField layout="stacked" label={t('infrastructure.clusterInfo.hosts')} value={presentation.hosts.length.toString()} />
          <KeyValueField layout="stacked" label={t('infrastructure.clusterInfo.relations')} value={presentation.relationCount.toString()} />
          <KeyValueField layout="stacked" label={t('infrastructure.clusterInfo.version')} value={presentation.versionEvidence.join(' | ') || t('infrastructure.values.unavailable')} />
        </div>
      </PanelFrame>
      <PanelFrame className="overflow-hidden bg-forensics-surface p-0">
        <SectionHeader icon={Server} title={t('infrastructure.clusterInfo.hostEvidence')} subtitle={t('infrastructure.clusterInfo.hostEvidenceDescription')} className="px-4 py-3" />
        <div className="overflow-x-auto">
          <div className="grid min-w-[760px] grid-cols-[minmax(180px,1fr)_140px_minmax(260px,2fr)_130px] gap-x-4 border-b border-forensics-border bg-forensics-panel px-4 py-2 text-[10px] uppercase tracking-wide text-forensics-muted">
            <span>{t('infrastructure.clusterInfo.host')}</span><span>{t('infrastructure.clusterInfo.hostname')}</span><span>{t('infrastructure.clusterInfo.version')}</span><span>{t('infrastructure.clusterInfo.identity')}</span>
          </div>
          <div className="divide-y divide-forensics-border-light">
            {presentation.hosts.map(({ node, fact }) => (
              <div key={node.id} className="grid min-w-[760px] grid-cols-[minmax(180px,1fr)_140px_minmax(260px,2fr)_130px] gap-x-4 px-4 py-3 text-xs">
                <span className="truncate text-forensics-text" title={node.name}>{node.name}</span>
                <span className="truncate text-forensics-text-secondary">{fact?.hostname ?? t('infrastructure.values.unavailable')}</span>
                <span className="truncate font-mono text-forensics-text-secondary">{formatHostVersion(fact) ?? t('infrastructure.values.unavailable')}</span>
                <span className="text-forensics-muted">{stateLabel(node.confidence, t)}</span>
              </div>
            ))}
            {presentation.hosts.length === 0 ? <div className="px-4 py-6 text-xs text-forensics-muted">{t('infrastructure.values.notFound')}</div> : null}
          </div>
        </div>
      </PanelFrame>
    </div>
  );
}
