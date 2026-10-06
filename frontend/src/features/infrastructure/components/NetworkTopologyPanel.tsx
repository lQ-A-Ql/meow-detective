import { ChevronRight, Network } from 'lucide-react';
import type { TFunction } from 'i18next';
import { EmptyState, PanelFrame, SectionHeader } from '@/components/data-display';
import type { InfrastructureGraphEdge, InfrastructureGraphNode } from '@/types/models';
import type { InfrastructureNetworkFact } from '@/types/infrastructure';
import { NetworkTopologyCanvas } from './NetworkTopologyCanvas';
import { relationLabel } from '../logic/labels';
import { VirtualList } from '@/components/lists/VirtualList';
import { keyedItems } from '@/lib/keyed-items';
import { useMemo } from 'react';

export function NetworkTopologyPanel({
  nodes,
  edges,
  nodeNames,
  selectedId,
  onSelect,
  t,
  networkFacts,
}: {
  nodes: InfrastructureGraphNode[];
  edges: InfrastructureGraphEdge[];
  nodeNames: Map<string, string>;
  selectedId?: string;
  onSelect: (id: string) => void;
  t: TFunction;
  networkFacts: InfrastructureNetworkFact[];
}) {
  return (
    <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_360px]">
      <PanelFrame className="overflow-hidden bg-forensics-surface p-4">
        <div className="mb-4 flex items-center gap-2 text-xs text-forensics-muted"><Network size={14} />{t('infrastructure.topologyHint')}</div>
        <NetworkTopologyCanvas nodes={nodes} edges={edges} selectedId={selectedId} onSelect={onSelect} t={t} />
      </PanelFrame>
      <div className="space-y-4"><RelationEvidenceList edges={edges} nodeNames={nodeNames} t={t} /><NetworkFactList facts={networkFacts} t={t} /></div>
    </div>
  );
}

function NetworkFactList({ facts, t }: { facts: InfrastructureNetworkFact[]; t: TFunction }) {
  return <PanelFrame className="min-h-0 overflow-hidden bg-forensics-surface p-0">
    <SectionHeader icon={Network} title={t('infrastructure.networkFacts.title')} subtitle={t('infrastructure.networkFacts.description')} className="px-4 py-3" />
    {facts.length === 0 ? <EmptyState>{t('infrastructure.networkFacts.empty')}</EmptyState> :
      <VirtualList items={facts} getItemKey={getFactKey} estimateSize={100} style={{ height: 'min(45vh, 360px)' }}
        renderItem={(fact) => <div className="border-b border-forensics-border px-4 py-3 text-[11px]">
          <div className="flex items-center justify-between gap-3"><span className="text-forensics-text">{t(`infrastructure.networkFacts.kinds.${fact.factKind}`)}</span><span className="font-mono text-[10px] text-forensics-muted">{fact.confidence}</span></div>
          <div className="mt-1 break-words font-mono text-forensics-text-secondary">{fact.subject}: {fact.value}</div>
          <div className="mt-1 truncate text-[10px] text-forensics-muted" title={fact.sourcePath}>{fact.sourcePath}:{fact.lineNumber}</div>
        </div>} />}
  </PanelFrame>;
}

const getFactKey = (fact: InfrastructureNetworkFact) => fact.id;
const getRelationKey = (edge: InfrastructureGraphEdge) => JSON.stringify([edge.sourceId, edge.targetId, edge.relationKind, edge.provenanceJson]);
const getKeyedRelationKey = (row: { key: string }) => row.key;

export function RelationEvidenceList({ edges, nodeNames, t }: { edges: InfrastructureGraphEdge[]; nodeNames: Map<string, string>; t: TFunction }) {
  const rows = useMemo(() => keyedItems(edges, getRelationKey), [edges]);
  return (
    <PanelFrame className="min-h-0 overflow-hidden bg-forensics-surface p-0">
      <SectionHeader icon={Network} title={t('infrastructure.sections.relations')} subtitle={t('infrastructure.relationsDescription')} className="px-4 py-3" />
      {rows.length > 0 ? <VirtualList items={rows} getItemKey={getKeyedRelationKey} estimateSize={80}
        style={{ height: 'min(60vh, 540px)' }} renderItem={({ item: edge }) => (
          <div className="grid gap-2 border-b border-forensics-border px-4 py-3 sm:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] sm:items-center">
            <div className="truncate text-xs text-forensics-text" title={nodeNames.get(edge.sourceId) ?? edge.sourceId}>{nodeNames.get(edge.sourceId) ?? t('infrastructure.values.unknown')}</div>
            <div className="flex items-center gap-1 text-[10px] text-forensics-muted"><ChevronRight size={13} /><span>{relationLabel(edge.relationKind, t)}</span></div>
            <div className="truncate text-xs text-forensics-text" title={nodeNames.get(edge.targetId) ?? edge.targetId}>{nodeNames.get(edge.targetId) ?? t('infrastructure.values.unknown')}</div>
          </div>
        )} /> : null}
      {edges.length === 0 ? <EmptyState>{t('infrastructure.values.notFound')}</EmptyState> : null}
    </PanelFrame>
  );
}
