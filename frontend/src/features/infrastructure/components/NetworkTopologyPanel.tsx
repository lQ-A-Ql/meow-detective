import { ChevronRight, Network } from 'lucide-react';
import type { TFunction } from 'i18next';
import { EmptyState, PanelFrame, SectionHeader } from '@/components/data-display';
import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';
import { NetworkTopologyCanvas } from './NetworkTopologyCanvas';
import { relationLabel } from '../logic/labels';

export function NetworkTopologyPanel({
  nodes,
  edges,
  nodeNames,
  selectedId,
  onSelect,
  t,
}: {
  nodes: InfrastructureGraphNode[];
  edges: InfrastructureGraphEdge[];
  nodeNames: Map<string, string>;
  selectedId?: string;
  onSelect: (id: string) => void;
  t: TFunction;
}) {
  return (
    <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
      <PanelFrame className="overflow-hidden bg-forensics-surface p-4">
        <div className="mb-4 flex items-center gap-2 text-xs text-forensics-muted"><Network size={14} />{t('infrastructure.topologyHint')}</div>
        <NetworkTopologyCanvas nodes={nodes} edges={edges} selectedId={selectedId} onSelect={onSelect} t={t} />
      </PanelFrame>
      <RelationEvidenceList edges={edges} nodeNames={nodeNames} t={t} />
    </div>
  );
}

export function RelationEvidenceList({ edges, nodeNames, t }: { edges: InfrastructureGraphEdge[]; nodeNames: Map<string, string>; t: TFunction }) {
  return (
    <PanelFrame className="min-h-0 overflow-hidden bg-forensics-surface p-0">
      <SectionHeader icon={Network} title={t('infrastructure.sections.relations')} subtitle={t('infrastructure.relationsDescription')} className="px-4 py-3" />
      <div className="max-h-[540px] divide-y divide-forensics-border overflow-auto scrollbar-none">
        {edges.map((edge, index) => (
          <div key={`${edge.sourceId}-${edge.targetId}-${index}`} className="grid gap-2 px-4 py-3 sm:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] sm:items-center">
            <div className="truncate text-xs text-forensics-text" title={nodeNames.get(edge.sourceId) ?? edge.sourceId}>{nodeNames.get(edge.sourceId) ?? t('infrastructure.values.unknown')}</div>
            <div className="flex items-center gap-1 text-[10px] text-forensics-muted"><ChevronRight size={13} /><span>{relationLabel(edge.relationKind, t)}</span></div>
            <div className="truncate text-xs text-forensics-text" title={nodeNames.get(edge.targetId) ?? edge.targetId}>{nodeNames.get(edge.targetId) ?? t('infrastructure.values.unknown')}</div>
          </div>
        ))}
      </div>
      {edges.length === 0 ? <EmptyState>{t('infrastructure.values.notFound')}</EmptyState> : null}
    </PanelFrame>
  );
}
