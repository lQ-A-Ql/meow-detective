import type { CapabilityKind, CapabilityMatrix, CapabilityState, SourceCapability } from '@/types/models';

/**
 * States that let an investigator open the catalog (browse/search/emulation).
 * `partial` shows only committed partitions/rows but stays navigable.
 */
export const CATALOG_OPEN_STATES: ReadonlySet<CapabilityState> = new Set(['ready', 'partial']);

/** States that mean a capability task is currently making progress. */
export const ACTIVE_CAPABILITY_STATES: ReadonlySet<CapabilityState> = new Set(['queued', 'running']);

/** Find a single capability row for a source; undefined when the matrix has no row. */
export function findSourceCapability(
  matrix: CapabilityMatrix | undefined,
  dataSourceId: string,
  capability: CapabilityKind,
): SourceCapability | undefined {
  return matrix?.sources.find((row) => row.dataSourceId === dataSourceId && row.capability === capability);
}

/** Read the capability state for a source without throwing on a missing row. */
export function capabilityStateFor(
  matrix: CapabilityMatrix | undefined,
  dataSourceId: string,
  capability: CapabilityKind,
): CapabilityState | undefined {
  return findSourceCapability(matrix, dataSourceId, capability)?.state;
}

/** Admit a source only when its current capability row allows the operation. */
export function isSourceCapabilityEligible(
  matrix: CapabilityMatrix | undefined,
  dataSourceId: string,
  capability: CapabilityKind,
  allowStates: ReadonlySet<CapabilityState> = CATALOG_OPEN_STATES,
): boolean {
  const row = findSourceCapability(matrix, dataSourceId, capability);
  return row !== undefined && allowStates.has(row.state);
}

export function isCapabilityActive(state: CapabilityState | undefined): boolean {
  return state !== undefined && ACTIVE_CAPABILITY_STATES.has(state);
}

export function isCapabilityPartial(state: CapabilityState | undefined): boolean {
  return state === 'partial';
}

export function isCapabilityDeferred(state: CapabilityState | undefined): boolean {
  return state === 'deferred';
}

export function isCapabilityFailed(state: CapabilityState | undefined): boolean {
  return state === 'failed';
}

/**
 * Pick the most urgent non-ready timeline row for the case-wide banner:
 * failed > deferred > running/queued > partial > ready. Returns undefined
 * when every timeline row is ready or the matrix has no timeline rows.
 */
export function mostUrgentTimelineRow(matrix: CapabilityMatrix | undefined): SourceCapability | undefined {
  const rows = (matrix?.sources ?? []).filter((row) => row.capability === 'timeline');
  const rank = (state: CapabilityState): number => {
    switch (state) {
      case 'failed':
        return 0;
      case 'deferred':
        return 1;
      case 'queued':
      case 'running':
        return 2;
      case 'partial':
        return 3;
      default:
        return 4;
    }
  };
  return rows
    .filter((row) => row.state !== 'ready' && row.state !== 'notApplicable')
    .sort((left, right) => rank(left.state) - rank(right.state))[0];
}
