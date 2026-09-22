import { Button } from '@/app/components/ui/button';
import { EDGE_TYPE_KEYS } from '@/features/graph/logic/graph-utils';
import { useTranslation } from 'react-i18next';
import type { GraphEdge, GraphNode, GraphProvenanceEntry } from '@/types/models';

interface GraphEdgeDetailsProps {
  edge: GraphEdge;
  nodeMap: Map<string, GraphNode>;
  provenance?: GraphProvenanceEntry[];
  provenanceLoading: boolean;
  onClose: () => void;
}

export function GraphEdgeDetails({ edge, nodeMap, provenance, provenanceLoading, onClose }: GraphEdgeDetailsProps) {
  const { t } = useTranslation();
  const source = nodeMap.get(edge.sourceId);
  const target = nodeMap.get(edge.targetId);
  return (
    <div className="space-y-3 text-[11px]">
      <div className="font-light text-forensics-text">{t(EDGE_TYPE_KEYS[edge.edgeType])}</div>
      <div className="rounded-none border border-forensics-border bg-forensics-surface p-1.5 text-center">
        <div className="text-[10px] text-forensics-muted">{t('graph.details.confidence')}</div>
        <div className="font-light text-forensics-text">{edge.confidence ?? '-'}</div>
      </div>
      <div className="space-y-1">
        <div className="text-[10px] text-forensics-muted">{t('graph.details.source')}</div>
        <div className="break-all rounded-none border border-forensics-border bg-forensics-surface p-1.5 font-mono text-[10px]">{source?.label ?? edge.sourceId}</div>
      </div>
      <div className="space-y-1">
        <div className="text-[10px] text-forensics-muted">{t('graph.details.target')}</div>
        <div className="break-all rounded-none border border-forensics-border bg-forensics-surface p-1.5 font-mono text-[10px]">{target?.label ?? edge.targetId}</div>
      </div>
      <div className="space-y-1">
        <div className="text-[10px] text-forensics-muted">{t('graph.details.provenance')}</div>
        {provenanceLoading ? <div className="text-forensics-muted">{t('graph.loading')}</div> : null}
        {!provenanceLoading && provenance?.length ? (
          <div className="space-y-1">
            {provenance.map((entry, index) => (
              <div key={`${entry.edgeId}-${index}`} className="rounded-none border border-forensics-border bg-forensics-surface p-1.5">
                {entry.sourceParser ? <div>{t('graph.details.parser')}: {entry.sourceParser}</div> : null}
                {entry.sourceRuleId ? <div>{t('graph.details.rule')}: {entry.sourceRuleId}</div> : null}
                {entry.parserVersion ? <div>{t('graph.details.version')}: {entry.parserVersion}</div> : null}
              </div>
            ))}
          </div>
        ) : null}
        {!provenanceLoading && !provenance?.length ? <div className="text-forensics-muted">{t('graph.details.noProvenance')}</div> : null}
      </div>
      <Button type="button" size="sm" variant="ghost" onClick={onClose} className="h-6 text-[10px]">{t('graph.actions.close')}</Button>
    </div>
  );
}
