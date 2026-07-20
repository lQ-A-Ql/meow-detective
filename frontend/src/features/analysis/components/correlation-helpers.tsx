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
      return 'border-forensics-success-border bg-forensics-success-bg text-forensics-success-text';
    case 'strong':
      return 'border-forensics-info-border bg-forensics-info-bg text-forensics-info-text';
    case 'weak':
      return 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text';
    case 'heuristic':
      return 'border-forensics-border-strong bg-forensics-panel text-forensics-text-secondary';
    default:
      return 'border-forensics-border bg-forensics-surface text-forensics-text-tertiary';
  }
}

export function coverageTone(value: CorrelationCoverageStatus) {
  switch (value) {
    case 'covered':
      return 'border-forensics-success-border bg-forensics-success-bg text-forensics-success-text';
    case 'review':
      return 'border-forensics-warning-border bg-forensics-warning-bg text-forensics-warning-text';
    case 'missing':
      return 'border-forensics-border-strong bg-forensics-panel text-forensics-text-secondary';
    default:
      return 'border-forensics-border bg-forensics-surface text-forensics-text-tertiary';
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
          className="rounded-none border border-forensics-border-strong bg-forensics-surface px-2 py-1 text-[10px] font-mono text-forensics-text-secondary"
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
