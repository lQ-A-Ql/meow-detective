
import type { TFunction } from 'i18next';
import { HardDrive } from 'lucide-react';
import { Button } from '@/app/components/ui/button';
import { EmptyState, PanelFrame } from '@/components/data-display';
import type { InfrastructureGraphNode } from '@/types/models';
import { kindLabel, stateLabel } from '../logic/labels';

export function StorageEvidencePanel({ nodes, selectedId, onSelect, t }: { nodes: InfrastructureGraphNode[]; selectedId?: string; onSelect: (id: string) => void; t: TFunction }) {
  return (
    <PanelFrame className="overflow-hidden bg-forensics-surface p-0">
      <div className="grid min-w-0 md:grid-cols-2">
        {nodes.map((node) => <Button key={node.id} type="button" variant="forensicsGhost" shape="row" onClick={() => onSelect(node.id)} data-active={selectedId === node.id} className="h-auto min-h-20 min-w-0 border-b border-forensics-border px-4 py-3 data-[active=true]:border-l-2 data-[active=true]:border-l-forensics-primary-blue data-[active=true]:bg-forensics-hover md:[&:nth-child(odd)]:border-r"><div className="min-w-0 text-left"><div className="flex min-w-0 items-center gap-2 text-xs text-forensics-text"><HardDrive size={14} /><span className="truncate">{node.name}</span></div><div className="mt-2 truncate text-[11px] text-forensics-muted">{kindLabel(node.kind, t)}</div></div><span className="shrink-0 text-[10px] text-forensics-muted">{stateLabel(node.status, t)}</span></Button>)}
      </div>
      {nodes.length === 0 ? <EmptyState className="mt-4">{t('infrastructure.storage.empty')}</EmptyState> : null}
    </PanelFrame>
  );
}
