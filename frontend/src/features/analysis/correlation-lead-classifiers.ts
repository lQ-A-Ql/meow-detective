import type { CorrelationLead } from '@/types/models';
export function isReviewLead(lead: CorrelationLead) {
  return lead.caveats.length > 0 || lead.confidence === 'weak' || lead.confidence === 'heuristic' || lead.provenance.some((item) => item.guaranteeLevel === 'experimental' || item.guaranteeLevel === 'notGuaranteed');
}
export function isHighConfidenceLead(lead: CorrelationLead) { return lead.confidence === 'direct' || lead.confidence === 'strong'; }
