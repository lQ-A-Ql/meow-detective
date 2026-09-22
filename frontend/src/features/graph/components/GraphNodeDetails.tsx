import { Button } from '@/app/components/ui/button';
import { NODE_TYPE_KEYS } from '@/features/graph/logic/graph-utils';
import { useTranslation } from 'react-i18next';
import type { GraphEdge, GraphNode, NodeType } from '@/types/models';

interface GraphNodeDetailsProps {
  node: GraphNode;
  edges: GraphEdge[];
  onExpand: (depth: number) => void;
  onClose: () => void;
}

export function GraphNodeDetails({ node, edges, onExpand, onClose }: GraphNodeDetailsProps) {
  const { t } = useTranslation();
  const degree = edges.filter((edge) => edge.sourceId === node.id || edge.targetId === node.id).length;
  return (
    <div className="space-y-3 text-[11px]">
      <div className="font-light text-forensics-text">{node.label || t('graph.values.untitled')}</div>
      <div className="grid grid-cols-2 gap-2">
        <div className="rounded-none border border-forensics-border bg-forensics-surface p-1.5 text-center">
          <div className="text-[10px] text-forensics-muted">{t('graph.details.type')}</div>
          <div className="font-light text-forensics-text">{t(NODE_TYPE_KEYS[node.nodeType as NodeType] ?? 'graph.values.unknown')}</div>
        </div>
        <div className="rounded-none border border-forensics-border bg-forensics-surface p-1.5 text-center">
          <div className="text-[10px] text-forensics-muted">{t('graph.details.degree')}</div>
          <div className="font-light text-forensics-text">{degree}</div>
        </div>
      </div>
      <div className="break-all rounded-none border border-forensics-border bg-forensics-surface p-2 font-mono text-[10px] text-forensics-text-secondary">{node.id}</div>
      <div className="break-words rounded-none border border-forensics-border bg-forensics-surface p-2 text-forensics-text-secondary">{node.summary || t('graph.values.noSummary')}</div>
      <div className="flex flex-wrap gap-1.5">
        <Button type="button" size="sm" variant="outline" onClick={() => onExpand(1)} className="h-6 text-[10px]">{t('graph.actions.expandOne')}</Button>
        <Button type="button" size="sm" variant="outline" onClick={() => onExpand(2)} className="h-6 text-[10px]">{t('graph.actions.expandTwo')}</Button>
        <Button type="button" size="sm" variant="ghost" onClick={onClose} className="h-6 text-[10px]">{t('graph.actions.close')}</Button>
      </div>
    </div>
  );
}
