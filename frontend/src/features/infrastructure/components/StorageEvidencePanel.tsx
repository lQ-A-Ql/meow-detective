import { HardDrive } from 'lucide-react';
import type { TFunction } from 'i18next';
import { Button } from '@/app/components/ui/button';
import { EmptyState, PanelFrame } from '@/components/data-display';
import type { InfrastructureGraphNode } from '@/types/models';
import { kindLabel, stateLabel } from '../logic/labels';

export function StorageEvidencePanel({ nodes, selectedId, onSelect, t }: { nodes: InfrastructureGraphNode[]; selectedId?: string; onSelect: (id: string) => void; t: TFunction }) {
  return (
    <PanelFrame className="bg-forensics-surface">
      <div className="mt-4 grid gap-3 md:grid-cols-2 xl:grid-cols-3">
        {nodes.map((node) => <Button key={node.id} type="button" variant="forensicsSurface" shape="card" onClick={() => onSelect(node.id)} data-active={selectedId === node.id} className="min-h-20 data-[active=true]:border-l-forensics-primary-blue data-[active=true]:bg-forensics-hover"><div className="min-w-0"><div className="flex items-center gap-2 text-xs text-forensics-text"><HardDrive size={14} /><span className="truncate">{node.name}</span></div><div className="mt-2 text-[11px] text-forensics-muted">{kindLabel(node.kind, t)}</div></div><span className="shrink-0 text-[10px] text-forensics-muted">{stateLabel(node.status, t)}</span></Button>)}
      </div>
      {nodes.length === 0 ? <EmptyState className="mt-4">{t('infrastructure.storage.empty')}</EmptyState> : null}
    </PanelFrame>
  );
}
