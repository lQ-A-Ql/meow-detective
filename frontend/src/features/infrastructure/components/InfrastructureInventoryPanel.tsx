import type { TFunction } from 'i18next';
import { HardDrive } from 'lucide-react';
import { useCallback, useMemo } from 'react';
import { PanelFrame, SectionHeader } from '@/components/data-display';
import { DenseDataTable, type DenseColumn } from '@/components/tables/DenseDataTable';
import { DenseDataTableFrame } from '@/components/tables/DenseDataTableFrame';
import { StatusBadge } from '@/components/status/StatusBadge';
import type { InfrastructureGraphNode } from '@/types/models';
import { domainLabel, kindLabel, stateLabel } from '../logic/labels';

const getNodeKey = (node: InfrastructureGraphNode) => node.id;
function statusVariant(value: string) {
  return value === 'ready' || value === 'complete' || value === 'verified' ? 'default' : 'outline';
}

export function InfrastructureInventoryPanel({ nodes, selectedId, onSelect, t }: { nodes: InfrastructureGraphNode[]; selectedId?: string; onSelect: (id: string) => void; t: TFunction }) {
  const columns = useMemo<DenseColumn<InfrastructureGraphNode>[]>(() => [
    { key: 'name', title: t('infrastructure.columns.name'), className: 'w-[36%]', render: (node) => <span title={node.name}>{node.name}</span> },
    { key: 'domain', title: t('infrastructure.columns.layer'), render: (node) => domainLabel(node.domain, t) },
    { key: 'kind', title: t('infrastructure.columns.type'), render: (node) => kindLabel(node.kind, t) },
    { key: 'status', title: t('infrastructure.columns.status'), render: (node) => <StatusBadge label={stateLabel(node.status, t)} variant={statusVariant(node.status)} /> },
    { key: 'confidence', title: t('infrastructure.columns.confidence'), render: (node) => <StatusBadge label={stateLabel(node.confidence, t)} variant={statusVariant(node.confidence)} /> },
  ], [t]);
  const selectNode = useCallback((node: InfrastructureGraphNode) => onSelect(node.id), [onSelect]);

  return (
    <PanelFrame className="flex min-h-0 flex-col overflow-hidden bg-forensics-surface p-0">
      <SectionHeader icon={HardDrive} title={t('infrastructure.sections.inventory')} subtitle={t('infrastructure.inventoryDescription', { count: nodes.length })} className="px-4 py-3" />
      <DenseDataTableFrame rowCount={nodes.length} maxHeight="compact" variant="plain">
        <DenseDataTable
          columns={columns}
          rows={nodes}
          getRowKey={getNodeKey}
          selectedRowKey={selectedId}
          onRowClick={selectNode}
          horizontalScroll
          minColumnWidth={152}
          emptyTitle={t('infrastructure.values.notFound')}
          emptyDescription=""
        />
      </DenseDataTableFrame>
    </PanelFrame>
  );
}
