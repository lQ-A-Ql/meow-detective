import type {
  CorrelationCluster,
  CorrelationFamilyCoverage,
  CorrelationLead,
} from '@/types/models';
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
    <div className="rounded border border-[#e0e0e0] bg-white p-4" data-testid="correlation-family-coverage-panel">
      <div className="mb-3 flex items-center justify-between gap-3">
        <div>
          <div className="text-[12px] font-semibold text-[#111]">规则家族覆盖</div>
          <div className="mt-1 text-[11px] text-[#666]">
            直接展示关联快照产出的家族覆盖、线索强度与示例信号。
          </div>
        </div>
      </div>
      <div className="grid grid-cols-1 gap-3 2xl:grid-cols-2">
        {items.map((item) => (
          <div
            key={item.family}
            className="rounded border border-[#e5e7eb] bg-[#fcfcfc] p-3"
            data-testid={`correlation-family-${item.family}`}
          >
            <div className="flex items-center justify-between gap-2">
              <div>
                <div className="text-[12px] font-medium text-[#111]">{item.displayName}</div>
                <div className="mt-1 font-mono text-[10px] text-[#888]">{item.family}</div>
              </div>
              <span className={`rounded border px-2 py-0.5 text-[10px] font-mono ${coverageTone(item.status)}`}>
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
              <div className="mt-3 rounded border border-[#e5e7eb] bg-white px-3 py-2 text-[11px] text-[#555]">
                <div className="mb-1 text-[10px] uppercase tracking-wider text-[#888]">Sample Signals</div>
                <div className="space-y-1">
                  {item.sampleSignals.map((signal) => (
                    <div key={`${item.family}-${signal}`}>{signal}</div>
                  ))}
                </div>
              </div>
            ) : (
              <div className="mt-3 text-[11px] text-[#777]">当前没有可展示的该家族命中信号。</div>
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
      className={`rounded border p-4 transition-colors ${
        selected
          ? 'border-[#111] bg-[#f7f7f7] shadow-[inset_0_0_0_1px_rgba(17,17,17,0.08)]'
          : 'border-[#e0e0e0] bg-[#fcfcfc] hover:border-[#cfcfcf]'
      }`}
    >
      <div className="flex items-start justify-between gap-3">
        <div>
          <div className="text-[13px] font-semibold text-[#111]">{lead.title}</div>
          <div className="mt-1 text-[11px] text-[#555]">{lead.summary}</div>
        </div>
        <span className={`rounded border px-2 py-0.5 text-[10px] font-mono ${confidenceTone(lead.confidence)}`}>
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
        <div className="mt-3 rounded border border-[#e5e7eb] bg-white px-3 py-2 text-[11px] text-[#555]">
          <div className="mb-1 text-[10px] uppercase tracking-wider text-[#888]">Match Signals</div>
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
            className="rounded border border-[#ddd] bg-white px-2 py-1 text-[10px] text-[#555]"
          >
            {item.sourceLabel}
          </span>
        ))}
      </div>
      <div className="mt-3 flex flex-wrap gap-2">
        {lead.jumps.map((jump) => (
          <button
            key={`${lead.id}-${jump.route}-${jump.targetId}`}
            type="button"
            onClick={(event) => {
              event.stopPropagation();
              onJump(jump.route, jump.targetId);
            }}
            className="rounded border border-[#ddd] bg-white px-2 py-1 text-[10px] text-[#555] hover:border-[#bbb] hover:bg-[#f7f7f7] hover:text-[#111]"
          >
            {jump.label}
          </button>
        ))}
      </div>
      {lead.provenance.length > 0 ? (
        <div className="mt-3 space-y-2 text-[11px] text-[#555]">
          {lead.provenance.slice(0, 3).map((item) => (
            <div
              key={`${lead.id}-${item.sourceKind}-${item.sourceRecordId}`}
              className="rounded border border-[#eee] bg-white px-3 py-2"
            >
              <div className="flex items-center justify-between gap-2">
                <span className="font-medium text-[#111]">{item.sourceLabel}</span>
                <span className="font-mono text-[10px] text-[#888]">{translateGuarantee(item.guaranteeLevel)}</span>
              </div>
              <div className="mt-1 break-all text-[#666]">
                {item.sourceKind} · {item.sourceRecordId}
                {item.producer ? ` · ${item.producer}` : ''}
              </div>
            </div>
          ))}
        </div>
      ) : null}
      {lead.caveats.length > 0 ? (
        <div className="mt-3 rounded border border-amber-200 bg-amber-50 p-3 text-[11px] text-amber-900">
          {lead.caveats.map((item) => (
            <div key={`${lead.id}-${item}`}>{item}</div>
          ))}
        </div>
      ) : null}
    </div>
  );
}
