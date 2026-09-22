import type { TFunction } from 'i18next';
import { HardDrive } from 'lucide-react';
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/app/components/ui/table';
import { EmptyState, PanelFrame, SectionHeader } from '@/components/data-display';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { InfrastructureGraphNode } from '@/types/models';
import { domainLabel, kindLabel, stateLabel } from '../logic/labels';

export function InfrastructureInventoryPanel({ nodes, selectedId, onSelect, t }: { nodes: InfrastructureGraphNode[]; selectedId?: string; onSelect: (id: string) => void; t: TFunction }) {
  return (
    <PanelFrame className="overflow-hidden bg-forensics-surface p-0">
      <SectionHeader icon={HardDrive} title={t('infrastructure.sections.inventory')} subtitle={t('infrastructure.inventoryDescription', { count: nodes.length })} className="px-4 py-3" />
      <div className="overflow-auto scrollbar-none">
        <Table className="min-w-[760px] text-left text-xs"><TableHeader><TableRow><TableHead>{t('infrastructure.columns.name')}</TableHead><TableHead>{t('infrastructure.columns.layer')}</TableHead><TableHead>{t('infrastructure.columns.type')}</TableHead><TableHead>{t('infrastructure.columns.status')}</TableHead><TableHead>{t('infrastructure.columns.confidence')}</TableHead></TableRow></TableHeader><TableBody>{nodes.map((node) => <TableRow key={node.id} data-state={selectedId === node.id ? 'selected' : undefined} onClick={() => onSelect(node.id)} className="cursor-pointer"><TableCell className="max-w-[280px] truncate text-forensics-text">{node.name}</TableCell><TableCell className="text-forensics-muted">{domainLabel(node.domain, t)}</TableCell><TableCell className="text-forensics-muted">{kindLabel(node.kind, t)}</TableCell><TableCell><StatusBadge label={stateLabel(node.status, t)} variant={statusVariant(node.status)} /></TableCell><TableCell><StatusBadge label={stateLabel(node.confidence, t)} variant={statusVariant(node.confidence)} /></TableCell></TableRow>)}</TableBody></Table>
      </div>
      {nodes.length === 0 ? <EmptyState>{t('infrastructure.values.notFound')}</EmptyState> : null}
    </PanelFrame>
  );
}

function statusVariant(value: string) { return value === 'ready' || value === 'complete' || value === 'verified' ? 'default' : 'outline'; }
