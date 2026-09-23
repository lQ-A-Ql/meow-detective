import { Boxes, Server } from 'lucide-react';
import type { TFunction } from 'i18next';
import { Button } from '@/app/components/ui/button';
import { EmptyState, PanelFrame, SectionHeader } from '@/components/data-display';
import type { InfrastructureGraphNode } from '@/types/models';
import type { InfrastructureNetworkFact } from '@/types/infrastructure';
import { kindLabel, stateLabel } from '../logic/labels';
import { KubernetesEvidencePanel } from './KubernetesEvidencePanel';

export function WorkloadEvidencePanel({
  nodes,
  selectedId,
  onSelect,
  t,
  networkFacts,
}: {
  nodes: InfrastructureGraphNode[];
  selectedId?: string;
  onSelect: (id: string) => void;
  t: TFunction;
  networkFacts: InfrastructureNetworkFact[];
}) {
  const groups = groupByKind(nodes);
  return (
    <div className="space-y-4">
      <KubernetesEvidencePanel facts={networkFacts} t={t} />
      <PanelFrame className="bg-forensics-surface">
        <SectionHeader icon={Boxes} title={t('infrastructure.workloads.title')} subtitle={t('infrastructure.workloads.description')} />
        <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {groups.map(([kind, items]) => (
            <div key={kind} className="border border-forensics-border bg-forensics-panel px-3 py-3">
              <div className="flex items-center justify-between gap-3"><div className="flex items-center gap-2 text-xs text-forensics-text"><Server size={14} />{kindLabel(kind, t)}</div><span className="font-mono text-[11px] text-forensics-muted">{items.length}</span></div>
              <div className="mt-3 space-y-1">
                {items.slice(0, 4).map((node) => <Button key={node.id} type="button" variant="forensicsGhost" size="inline" onClick={() => onSelect(node.id)} data-active={selectedId === node.id} className="h-auto w-full justify-between gap-3 border-l-2 border-transparent py-1 text-left text-[11px] data-[active=true]:border-forensics-primary-blue data-[active=true]:bg-forensics-hover"><span className="min-w-0 truncate">{node.name}</span><span className="shrink-0 text-[10px] text-forensics-muted">{stateLabel(node.status, t)}</span></Button>)}
              </div>
            </div>
          ))}
        </div>
        {groups.length === 0 ? <EmptyState className="mt-4">{t('infrastructure.workloads.empty')}</EmptyState> : null}
      </PanelFrame>
    </div>
  );
}

function groupByKind(nodes: InfrastructureGraphNode[]) {
  const groups = new Map<string, InfrastructureGraphNode[]>();
  for (const node of nodes) {
    const group = groups.get(node.kind) ?? [];
    group.push(node);
    groups.set(node.kind, group);
  }
  return [...groups.entries()].sort(([left], [right]) => left.localeCompare(right));
}
