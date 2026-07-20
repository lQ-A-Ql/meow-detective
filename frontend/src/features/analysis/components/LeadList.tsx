import type {
  CorrelationFamilyCoverage,
  CorrelationLead,
} from '@/types/models';
import { Button } from '@/app/components/ui/button';
import {
  confidenceLabel,
  confidenceTone,
  coverageLabel,
  coverageTone,
  FamilyPills,
  Metric,
  summarizeLeadKinds,
  translateGuarantee,
} from './correlation-helpers';

export function CorrelationFamilyCoveragePanel({ items }: { items: CorrelationFamilyCoverage[] }) {
  return (
    <div className="rounded-none border border-forensics-border bg-forensics-surface p-4" data-testid="correlation-family-coverage-panel">
      <div className="mb-3 flex items-center justify-between gap-3">
        <div>
          <div className="text-[12px] font-light text-forensics-text">规则家族覆盖</div>
          <div className="mt-1 text-[11px] text-forensics-muted">
            直接展示关联快照产出的家族覆盖、线索强度与命中信号。
          </div>
        </div>
      </div>
      <div className="grid grid-cols-1 gap-3 2xl:grid-cols-2">
        {items.map((item) => (
          <div
            key={item.family}
            className="rounded-none border border-forensics-border bg-forensics-surface p-3"
            data-testid={`correlation-family-${item.family}`}
          >
            <div className="flex items-center justify-between gap-2">
              <div>
                <div className="text-[12px] font-light text-forensics-text">{item.displayName}</div>
                <div className="mt-1 font-mono text-[10px] text-forensics-muted-light">{item.family}</div>
              </div>
              <span className={`rounded-none border px-2 py-0.5 text-[10px] font-mono ${coverageTone(item.status)}`}>
                {coverageLabel(item.status)}
              </span>
            </div>
            <div className="mt-3 grid grid-cols-2 gap-2 lg:grid-cols-4">
              <Metric label="Lead" value={item.leadCount.toString()} />
              <Metric label="高置信" value={item.highConfidenceLeadCount.toString()} />
              <Metric label="待复核" value={item.reviewLeadCount.toString()} />
              <Metric label="Cluster" value={item.clusterCount.toString()} />
            </div>
            {item.sampleSignals.length > 0 ? (
              <div className="mt-3 rounded-none border border-forensics-border bg-forensics-surface px-3 py-2 text-[11px] text-forensics-text-tertiary">
                <div className="mb-1 text-[10px] uppercase tracking-wider text-forensics-muted-light">Observed Signals</div>
                <div className="space-y-1">
                  {item.sampleSignals.map((signal) => (
                    <div key={`${item.family}-${signal}`}>{signal}</div>
                  ))}
                </div>
              </div>
            ) : (
              <div className="mt-3 text-[11px] text-forensics-muted">当前没有可展示的该家族命中信号。</div>
            )}
          </div>
        ))}
      </div>
    </div>
  );
}

export function LeadCard({
  lead,
  selected,
  onJump,
  onSelect,
}: {
  lead: CorrelationLead;
  selected: boolean;
  onJump: (route: string, targetId: string) => void;
  onSelect: () => void;
}) {
  return (
    <div
      role="button"
      tabIndex={0}
      aria-pressed={selected}
      data-testid={`lead-card-${lead.id}`}
      onClick={onSelect}
      onKeyDown={(event) => {
        if (event.key === 'Enter' || event.key === ' ') {
          event.preventDefault();
          onSelect();
        }
      }}
      className={`rounded-none border p-4 transition-colors ${
        selected
          ? 'border-forensics-text bg-forensics-panel shadow-none'
          : 'border-forensics-border bg-forensics-surface hover:border-forensics-border-strong'
      }`}
    >
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-[13px] font-light text-forensics-text">{lead.title}</div>
          <div className="mt-1 text-[11px] text-forensics-text-tertiary">{lead.summary}</div>
        </div>
        <span className={`rounded-none border px-2 py-0.5 text-[10px] font-mono ${confidenceTone(lead.confidence)}`}>
          {confidenceLabel(lead.confidence)}
        </span>
      </div>
      <div className="mt-3 grid grid-cols-2 gap-2 text-[11px]">
        <Metric label="主文件" value={lead.primaryFileId} mono />
        <Metric label="支撑节点" value={lead.supportingNodeIds.length.toString()} />
        <Metric label="来源类别" value={summarizeLeadKinds(lead) || '-'} />
        <Metric label="告警数" value={lead.caveats.length.toString()} />
      </div>
      <FamilyPills families={lead.families} testId={`lead-families-${lead.id}`} />
      {lead.matchSignals.length > 0 ? (
        <div className="mt-3 rounded-none border border-forensics-border bg-forensics-surface px-3 py-2 text-[11px] text-forensics-text-tertiary">
          <div className="mb-1 text-[10px] uppercase tracking-wider text-forensics-muted-light">Match Signals</div>
          <div className="space-y-1">
            {lead.matchSignals.map((item) => (
              <div key={`${lead.id}-${item}`}>{item}</div>
            ))}
          </div>
        </div>
      ) : null}
      <div className="mt-3 flex flex-wrap gap-2">
        {lead.provenance.map((item) => (
          <span
            key={`${lead.id}-${item.sourceKind}-${item.sourceRecordId}`}
            className="rounded-none border border-forensics-border bg-forensics-surface px-2 py-1 text-[10px] text-forensics-text-tertiary"
          >
            {item.sourceLabel}
          </span>
        ))}
      </div>
      <div className="mt-3 flex flex-wrap gap-2">
        {lead.jumps.map((jump) => (
          <Button
            key={`${lead.id}-${jump.route}-${jump.targetId}`}
            type="button"
            variant="forensicsOutline"
            size="compact"
            onClick={(event) => {
              event.stopPropagation();
              onJump(jump.route, jump.targetId);
            }}
            className="text-[10px]"
          >
            {jump.label}
          </Button>
        ))}
      </div>
      {lead.provenance.length > 0 ? (
        <div className="mt-3 space-y-2 text-[11px] text-forensics-text-tertiary">
          {lead.provenance.slice(0, 3).map((item) => (
            <div
              key={`${lead.id}-${item.sourceKind}-${item.sourceRecordId}`}
              className="rounded-none border border-forensics-border-light bg-forensics-surface px-3 py-2"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="font-light text-forensics-text">{item.sourceLabel}</span>
                <span className="font-mono text-[10px] text-forensics-muted-light">{translateGuarantee(item.guaranteeLevel)}</span>
              </div>
              <div className="mt-1 break-all text-forensics-muted">
                {item.sourceKind} · {item.sourceRecordId}
                {item.producer ? ` · ${item.producer}` : ''}
              </div>
            </div>
          ))}
        </div>
      ) : null}
      {lead.caveats.length > 0 ? (
        <div className="mt-3 rounded-none border border-forensics-warning-border bg-forensics-warning-bg p-3 text-[11px] text-forensics-warning-text">
          {lead.caveats.map((item) => (
            <div key={`${lead.id}-${item}`}>{item}</div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
