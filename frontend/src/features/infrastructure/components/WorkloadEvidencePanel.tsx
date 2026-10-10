import { Server } from 'lucide-react';
import { ListRow } from '@/components/lists/ListRow';
import type { TFunction } from 'i18next';

import { EmptyState, PanelFrame } from '@/components/data-display';
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
        <div className="mt-4 grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
          {groups.map(([kind, items]) => (
            <div key={kind} className="bg-transparent px-3 py-3">
              <div className="flex items-center justify-between gap-3"><div className="flex items-center gap-2 text-xs text-forensics-text"><Server size={14} />{kindLabel(kind, t)}</div><span className="font-mono text-[11px] text-forensics-muted">{items.length}</span></div>
              <div className="mt-3 space-y-1">
                {items.slice(0, 4).map((node) => <ListRow
                  key={node.id}
                  selected={selectedId === node.id}
                  onActivate={() => onSelect(node.id)}
                  ariaLabel={node.name}
                  className="h-6 gap-3 py-1 text-[11px]"
                  label={node.name}
                  meta={<span className="shrink-0 text-[10px] text-forensics-ink-3">{stateLabel(node.status, t)}</span>}
                />)}
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
