import { Button } from '@/app/components/ui/button';
import { NODE_LABELS } from '@/features/graph/components/graph-utils';
import type { GraphEdge, GraphNode, NodeType } from '@/types/models';

interface GraphNodeDetailsProps {
  node: GraphNode;
  edges: GraphEdge[];
  onExpand: (depth: number) => void;
  onClose: () => void;
}

export function GraphNodeDetails({ node, edges, onExpand, onClose }: GraphNodeDetailsProps) {
  const degree = edges.filter((edge) => edge.sourceId === node.id || edge.targetId === node.id).length;
  return (
    <div className="space-y-3 text-[11px]">
      <div className="font-light text-forensics-text">{node.label || '(无标签)'}</div>
      <div className="grid grid-cols-2 gap-2">
        <div className="rounded-none border border-forensics-border bg-forensics-surface p-1.5 text-center">
          <div className="text-[10px] text-forensics-muted">类型</div>
          <div className="font-light text-forensics-text">{NODE_LABELS[node.nodeType as NodeType] ?? node.nodeType}</div>
        </div>
        <div className="rounded-none border border-forensics-border bg-forensics-surface p-1.5 text-center">
          <div className="text-[10px] text-forensics-muted">度</div>
          <div className="font-light text-forensics-text">{degree}</div>
        </div>
      </div>
      <div className="break-all rounded-none border border-forensics-border bg-forensics-surface p-2 font-mono text-[10px] text-forensics-text-secondary">{node.id}</div>
      <div className="break-words rounded-none border border-forensics-border bg-forensics-surface p-2 text-forensics-text-secondary">{node.summary || '(无摘要)'}</div>
      <div className="flex flex-wrap gap-1.5">
        <Button type="button" size="sm" variant="outline" onClick={() => onExpand(1)} className="h-6 text-[10px]">展开 1 层</Button>
        <Button type="button" size="sm" variant="outline" onClick={() => onExpand(2)} className="h-6 text-[10px]">展开 2 层</Button>
        <Button type="button" size="sm" variant="ghost" onClick={onClose} className="h-6 text-[10px]">关闭</Button>
      </div>
    </div>
  );
}
