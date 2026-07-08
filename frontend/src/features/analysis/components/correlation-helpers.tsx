import type {
  CorrelationConfidence,
  CorrelationCoverageStatus,
  CorrelationLead,
} from '@/types/models';
import { MetricCard } from '@/components/data-display';

export function confidenceLabel(value: CorrelationConfidence) {
  switch (value) {
    case 'direct':
      return 'Direct';
    case 'strong':
      return 'Strong';
    case 'weak':
      return 'Weak';
    case 'heuristic':
      return 'Heuristic';
    default:
      return value;
  }
}

export function confidenceTone(value: CorrelationConfidence) {
  switch (value) {
    case 'direct':
      return 'border-[#0d7a32] bg-[#effaf2] text-[#0d7a32]';
    case 'strong':
      return 'border-[#175cd3] bg-[#eff6ff] text-[#175cd3]';
    case 'weak':
      return 'border-[#b54708] bg-[#fff7ed] text-[#b54708]';
    case 'heuristic':
      return 'border-[#667085] bg-[#f8fafc] text-[#475467]';
    default:
      return 'border-[#ddd] bg-white text-[#555]';
  }
}

export function coverageTone(value: CorrelationCoverageStatus) {
  switch (value) {
    case 'covered':
      return 'border-[#0d7a32] bg-[#effaf2] text-[#0d7a32]';
    case 'review':
      return 'border-[#b54708] bg-[#fff7ed] text-[#b54708]';
    case 'missing':
      return 'border-[#667085] bg-[#f8fafc] text-[#475467]';
    default:
      return 'border-[#ddd] bg-white text-[#555]';
  }
}

export function coverageLabel(value: CorrelationCoverageStatus) {
  switch (value) {
    case 'covered':
      return 'Covered';
    case 'review':
      return 'Review';
    case 'missing':
      return 'Missing';
    default:
      return value;
  }
}

export function translateGuarantee(value: string) {
  switch (value) {
    case 'guaranteed':
      return 'Guaranteed';
    case 'bestEffort':
      return 'BestEffort';
    case 'experimental':
      return 'Experimental';
    case 'notGuaranteed':
      return 'NotGuaranteed';
    default:
      return value;
  }
}

export function summarizeLeadKinds(lead: CorrelationLead) {
  if (lead.families.length > 0) {
    return lead.families.join(' / ');
  }
  const labels = lead.provenance.map((item) => item.sourceLabel);
  const uniqueLabels = Array.from(new Set(labels));
  if (uniqueLabels.length === 0) {
    return 'RuleMatch';
  }
  return uniqueLabels.join(' / ');
}

export function isReviewLead(lead: CorrelationLead) {
  return (
    lead.caveats.length > 0
    || lead.confidence === 'weak'
    || lead.confidence === 'heuristic'
    || lead.provenance.some(
      (item) => item.guaranteeLevel === 'experimental' || item.guaranteeLevel === 'notGuaranteed',
    )
  );
}

export function isHighConfidenceLead(lead: CorrelationLead) {
  return lead.confidence === 'direct' || lead.confidence === 'strong';
}

export function Metric({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return <MetricCard label={label} value={value} mono={mono} size="sm" />;
}

export function FamilyPills({ families, testId }: { families: string[]; testId: string }) {
  if (families.length === 0) {
    return null;
  }

  return (
    <div className="mt-3 flex flex-wrap gap-2" data-testid={testId}>
      {families.map((family) => (
        <span
          key={`${testId}-${family}`}
          className="rounded border border-[#d0d5dd] bg-white px-2 py-1 text-[10px] font-mono text-[#344054]"
        >
          {family}
        </span>
      ))}
    </div>
  );
}

export function OverviewCard({ label, value }: { label: string; value: string }) {
  return <MetricCard label={label} value={value} mono={false} align="center" size="md" />;
}
