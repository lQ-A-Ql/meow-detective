import { Button } from '@/app/components/ui/button';
import { EDGE_LABELS } from '@/features/graph/components/graph-utils';
import type { GraphEdge, GraphNode, GraphProvenanceEntry } from '@/types/models';

interface GraphEdgeDetailsProps {
  edge: GraphEdge;
  nodeMap: Map<string, GraphNode>;
  provenance?: GraphProvenanceEntry[];
  provenanceLoading: boolean;
  onClose: () => void;
}

export function GraphEdgeDetails({ edge, nodeMap, provenance, provenanceLoading, onClose }: GraphEdgeDetailsProps) {
  const source = nodeMap.get(edge.sourceId);
  const target = nodeMap.get(edge.targetId);
  return (
    <div className="space-y-3 text-[11px]">
      <div className="font-light text-forensics-text">{EDGE_LABELS[edge.edgeType]}</div>
      <div className="rounded-none border border-forensics-border bg-forensics-surface p-1.5 text-center">
        <div className="text-[10px] text-forensics-muted">置信度</div>
        <div className="font-light text-forensics-text">{edge.confidence ?? '-'}</div>
      </div>
      <div className="space-y-1">
        <div className="text-[10px] text-forensics-muted">源</div>
        <div className="break-all rounded-none border border-forensics-border bg-forensics-surface p-1.5 font-mono text-[10px]">{source?.label ?? edge.sourceId}</div>
      </div>
      <div className="space-y-1">
        <div className="text-[10px] text-forensics-muted">目标</div>
        <div className="break-all rounded-none border border-forensics-border bg-forensics-surface p-1.5 font-mono text-[10px]">{target?.label ?? edge.targetId}</div>
      </div>
      <div className="space-y-1">
        <div className="text-[10px] text-forensics-muted">来源追溯</div>
        {provenanceLoading ? <div className="text-forensics-muted">加载中...</div> : null}
        {!provenanceLoading && provenance?.length ? (
          <div className="space-y-1">
            {provenance.map((entry, index) => (
              <div key={`${entry.edgeId}-${index}`} className="rounded-none border border-forensics-border bg-forensics-surface p-1.5">
                {entry.sourceParser ? <div>解析器: {entry.sourceParser}</div> : null}
                {entry.sourceRuleId ? <div>规则: {entry.sourceRuleId}</div> : null}
                {entry.parserVersion ? <div>版本: {entry.parserVersion}</div> : null}
              </div>
            ))}
          </div>
        ) : null}
        {!provenanceLoading && !provenance?.length ? <div className="text-forensics-muted">无追溯记录</div> : null}
      </div>
      <Button type="button" size="sm" variant="ghost" onClick={onClose} className="h-6 text-[10px]">关闭</Button>
    </div>
  );
}
