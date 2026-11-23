import type { CapabilityState, SourceCapability } from '@/types/models';

const priority: Partial<Record<CapabilityState, number>> = {
  failed: 0,
  cancelled: 1,
  running: 2,
  queued: 2,
  partial: 3,
  deferred: 4,
  stale: 5,
  pending: 6,
};

export function pickArtifactsCapabilityAdvisory(rows: SourceCapability[]): SourceCapability | undefined {
  let selected: SourceCapability | undefined;
  let selectedRank = Number.POSITIVE_INFINITY;
  for (const row of rows) {
    if (row.capability !== 'artifacts') continue;
    const rank = priority[row.state];
    if (rank !== undefined && rank < selectedRank) {
      selected = row;
      selectedRank = rank;
    }
  }
  return selected;
}

export function artifactAdvisoryPresentation(row: SourceCapability) {
  const active = row.state === 'running' || row.state === 'queued';
  return {
    messageKey: `artifacts.capability.${active ? 'running' : row.state}`,
    active,
    retryable: !active && row.state !== 'ready' && row.state !== 'notApplicable',
    tone:
      row.state === 'failed'
        ? ('error' as const)
        : active || row.state === 'deferred'
          ? ('warning' as const)
          : ('neutral' as const),
  };
}
