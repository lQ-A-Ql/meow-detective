import type {
  CorrelationCluster,
  CorrelationLead,
  CorrelationSnapshot,
} from '@/types/models';
import { Button } from '@/app/components/ui/button';
import {
  confidenceLabel,
  confidenceTone,
  FamilyPills,
  Metric,
  translateGuarantee,
} from './correlation-helpers';

export function LeadDetailPanel({
  lead,
  primaryFileNode,
  supportingNodes,
  edges,
  relatedClusters,
  onJump,
}: {
  lead: CorrelationLead;
  primaryFileNode?: CorrelationSnapshot['nodes'][number];
  supportingNodes: CorrelationSnapshot['nodes'];
  edges: CorrelationSnapshot['edges'];
  relatedClusters: CorrelationCluster[];
  onJump: (route: string, targetId: string) => void;
}) {
  return (
    <div className="rounded-none border border-forensics-border bg-forensics-surface p-4" data-testid="selected-lead-panel">
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-[12px] uppercase tracking-wider text-forensics-muted-light">Lead 明细</div>
          <div className="mt-1 text-[15px] font-light text-forensics-text" data-testid="selected-lead-title">
            {lead.title}
          </div>
          <div className="mt-2 text-[11px] leading-5 text-forensics-text-tertiary">{lead.summary}</div>
        </div>
        <span className={`rounded-none border px-2 py-0.5 text-[10px] font-mono ${confidenceTone(lead.confidence)}`}>
          {confidenceLabel(lead.confidence)}
        </span>
      </div>

      <div className="mt-4 grid grid-cols-2 gap-2 text-[11px]">
        <Metric label="主文件" value={lead.primaryFileId} mono />
        <Metric label="支撑节点" value={lead.supportingNodeIds.length.toString()} />
        <Metric label="匹配信号" value={lead.matchSignals.length.toString()} />
        <Metric label="Provenance" value={lead.provenance.length.toString()} />
      </div>

      <FamilyPills families={lead.families} testId={`selected-lead-families-${lead.id}`} />

      <div className="mt-4 flex flex-wrap gap-2">
        {lead.jumps.map((jump) => (
          <Button
            key={`${lead.id}-detail-${jump.route}-${jump.targetId}`}
            type="button"
            variant="forensicsOutline"
            size="compact"
            onClick={() => onJump(jump.route, jump.targetId)}
            className="text-[10px]"
          >
            {jump.label}
          </Button>
        ))}
      </div>

      {lead.matchSignals.length > 0 ? (
        <div className="mt-4 rounded-none border border-forensics-border bg-forensics-surface px-3 py-3 text-[11px] text-forensics-text-tertiary">
          <div className="mb-2 text-[10px] uppercase tracking-wider text-forensics-muted-light">Match Signals</div>
          <div className="space-y-1">
            {lead.matchSignals.map((item) => (
              <div key={`${lead.id}-detail-signal-${item}`}>{item}</div>
            ))}
          </div>
        </div>
      ) : null}

      <div className="mt-4 grid grid-cols-1 gap-4 2xl:grid-cols-[1.05fr_0.95fr]">
        <div className="space-y-4">
          <div className="rounded-none border border-forensics-border-light bg-forensics-surface p-3">
            <div className="mb-2 text-[10px] uppercase tracking-wider text-forensics-muted-light">关联节点</div>
            <div className="space-y-2">
              {primaryFileNode ? (
                <NodeSummaryCard
                  node={primaryFileNode}
                  title="主文件节点"
                  onJump={onJump}
                />
              ) : null}
              {supportingNodes.length > 0 ? (
                supportingNodes.map((node) => (
                  <NodeSummaryCard
                    key={node.id}
                    node={node}
                    title="支撑节点"
                    onJump={onJump}
                  />
                ))
              ) : (
                <div className="text-[11px] text-forensics-muted">当前没有额外的支撑节点。</div>
              )}
            </div>
          </div>

          <div className="rounded-none border border-forensics-border-light bg-forensics-surface p-3">
            <div className="mb-2 text-[10px] uppercase tracking-wider text-forensics-muted-light">相关边</div>
            {edges.length > 0 ? (
              <div className="space-y-2">
                {edges.map((edge) => (
                  <div key={edge.id} className="rounded-none border border-forensics-border bg-forensics-surface px-3 py-2 text-[11px] text-forensics-text-tertiary">
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-mono text-[10px] text-forensics-muted-light">{edge.kind}</span>
                      <span className={`rounded-none border px-2 py-0.5 text-[10px] font-mono ${confidenceTone(edge.confidence)}`}>
                        {confidenceLabel(edge.confidence)}
                      </span>
                    </div>
                    <div className="mt-1 text-forensics-text">{edge.summary}</div>
                    <div className="mt-1 break-all text-[10px] text-forensics-muted">
                      {edge.fromNodeId} {'->'} {edge.toNodeId}
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-[11px] text-forensics-muted">当前 lead 尚未挂接可展示的关联边。</div>
            )}
          </div>
        </div>

        <div className="space-y-4">
          <div className="rounded-none border border-forensics-border-light bg-forensics-surface p-3">
            <div className="mb-2 text-[10px] uppercase tracking-wider text-forensics-muted-light">相关聚合</div>
            {relatedClusters.length > 0 ? (
              <div className="space-y-2">
                {relatedClusters.map((cluster) => (
                  <div key={cluster.id} className="rounded-none border border-forensics-border bg-forensics-surface px-3 py-2 text-[11px] text-forensics-text-tertiary">
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-light text-forensics-text">{cluster.title}</span>
                      <span className={`rounded-none border px-2 py-0.5 text-[10px] font-mono ${confidenceTone(cluster.confidence)}`}>
                        {confidenceLabel(cluster.confidence)}
                      </span>
                    </div>
                    <div className="mt-1">{cluster.summary}</div>
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-[11px] text-forensics-muted">当前没有同主文件的聚合 cluster。</div>
            )}
          </div>

          <div className="rounded-none border border-forensics-border-light bg-forensics-surface p-3">
            <div className="mb-2 text-[10px] uppercase tracking-wider text-forensics-muted-light">Provenance</div>
            {lead.provenance.length > 0 ? (
              <div className="space-y-2">
                {lead.provenance.map((item) => (
                  <div
                    key={`${lead.id}-detail-provenance-${item.sourceKind}-${item.sourceRecordId}`}
                    className="rounded-none border border-forensics-border bg-forensics-surface px-3 py-2 text-[11px] text-forensics-text-tertiary"
                  >
                    <div className="flex items-center justify-between gap-2">
                      <span className="font-light text-forensics-text">{item.sourceLabel}</span>
                      <span className="font-mono text-[10px] text-forensics-muted-light">{translateGuarantee(item.guaranteeLevel)}</span>
                    </div>
                    <div className="mt-1 break-all">
                      {item.sourceKind} · {item.sourceRecordId}
                      {item.producer ? ` · ${item.producer}` : ''}
                    </div>
                    {item.warningSummary.length > 0 ? (
                      <div className="mt-2 rounded-none border border-forensics-warning-border bg-forensics-warning-bg px-2 py-1 text-[10px] text-forensics-warning-text">
                        {item.warningSummary.join('；')}
                      </div>
                    ) : null}
                  </div>
                ))}
              </div>
            ) : (
              <div className="text-[11px] text-forensics-muted">当前没有可展示的 provenance。</div>
            )}
          </div>
        </div>
      </div>

      {lead.caveats.length > 0 ? (
        <div className="mt-4 rounded-none border border-forensics-warning-border bg-forensics-warning-bg p-3 text-[11px] text-forensics-warning-text">
          {lead.caveats.map((item) => (
            <div key={`${lead.id}-detail-caveat-${item}`}>{item}</div>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function NodeSummaryCard({
  node,
  title,
  onJump,
}: {
  node: CorrelationSnapshot['nodes'][number];
  title: string;
  onJump: (route: string, targetId: string) => void;
}) {
  return (
    <div className="rounded-none border border-forensics-border bg-forensics-surface px-3 py-2 text-[11px] text-forensics-text-tertiary">
      <div className="flex items-center justify-between gap-2">
        <span className="text-[10px] uppercase tracking-wider text-forensics-muted-light">{title}</span>
        <span className="font-mono text-[10px] text-forensics-muted-light">{node.kind}</span>
      </div>
      <div className="mt-1 font-light text-forensics-text">{node.title}</div>
      {node.subtitle ? <div className="mt-1 break-all">{node.subtitle}</div> : null}
      {node.badges.length > 0 ? (
        <div className="mt-2 flex flex-wrap gap-2">
          {node.badges.map((badge) => (
            <span key={`${node.id}-${badge}`} className="rounded-none border border-forensics-border bg-forensics-surface px-2 py-0.5 text-[10px] text-forensics-muted">
              {badge}
            </span>
          ))}
        </div>
      ) : null}
      {node.jumps.length > 0 ? (
        <div className="mt-2 flex flex-wrap gap-2">
          {node.jumps.map((jump) => (
            <Button
              key={`${node.id}-${jump.route}-${jump.targetId}`}
              type="button"
              variant="forensicsOutline"
              size="compact"
              onClick={() => onJump(jump.route, jump.targetId)}
              className="text-[10px]"
            >
              {jump.label}
            </Button>
          ))}
        </div>
      ) : null}
    </div>
  );
}
