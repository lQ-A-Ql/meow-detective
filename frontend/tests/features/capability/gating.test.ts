import { describe, expect, it } from 'vitest';
import {
  capabilityStateFor,
  findSourceCapability,
  isSourceCapabilityEligible,
  mostUrgentTimelineRow,
} from '@/features/capability/gating';
import type { CapabilityMatrix, SourceCapability } from '@/types/models';

function row(
  dataSourceId: string,
  capability: SourceCapability['capability'],
  state: SourceCapability['state'],
  extra: Partial<SourceCapability> = {},
): SourceCapability {
  return {
    dataSourceId,
    capability,
    state,
    revision: 1,
    derived: false,
    updatedAt: '2026-11-19T00:00:00Z',
    ...extra,
  };
}

function matrix(rows: SourceCapability[]): CapabilityMatrix {
  return { sources: rows };
}

describe('isSourceCapabilityEligible', () => {
  it('falls back to the legacy decision when the matrix is missing entirely', () => {
    expect(isSourceCapabilityEligible(undefined, 'source-1', 'catalog', true)).toBe(true);
    expect(isSourceCapabilityEligible(undefined, 'source-1', 'catalog', false)).toBe(false);
  });

  it('falls back to legacy when the matrix has no row for the source', () => {
    const m = matrix([row('source-other', 'catalog', 'ready')]);
    expect(isSourceCapabilityEligible(m, 'source-1', 'catalog', true)).toBe(true);
    expect(isSourceCapabilityEligible(m, 'source-1', 'catalog', false)).toBe(false);
  });

  it('falls back to legacy when the matrix source list is empty', () => {
    expect(isSourceCapabilityEligible(matrix([]), 'source-1', 'catalog', true)).toBe(true);
    expect(isSourceCapabilityEligible(matrix([]), 'source-1', 'catalog', false)).toBe(false);
  });

  it('a present pending row wins over a legacy-eligible source', () => {
    const m = matrix([row('source-1', 'catalog', 'pending')]);
    expect(isSourceCapabilityEligible(m, 'source-1', 'catalog', true)).toBe(false);
  });

  it('admits catalog ready and partial, rejects failed/deferred/pending', () => {
    const ready = matrix([row('s-ready', 'catalog', 'ready')]);
    const partial = matrix([row('s-partial', 'catalog', 'partial')]);
    const failed = matrix([row('s-failed', 'catalog', 'failed')]);
    const deferred = matrix([row('s-deferred', 'catalog', 'deferred')]);
    expect(isSourceCapabilityEligible(ready, 's-ready', 'catalog', false)).toBe(true);
    expect(isSourceCapabilityEligible(partial, 's-partial', 'catalog', false)).toBe(true);
    expect(isSourceCapabilityEligible(failed, 's-failed', 'catalog', true)).toBe(false);
    expect(isSourceCapabilityEligible(deferred, 's-deferred', 'catalog', true)).toBe(false);
  });

  it('scopes the lookup to the requested capability kind', () => {
    const m = matrix([row('source-1', 'search', 'ready'), row('source-1', 'catalog', 'pending')]);
    expect(findSourceCapability(m, 'source-1', 'catalog')?.state).toBe('pending');
    expect(capabilityStateFor(m, 'source-1', 'search')).toBe('ready');
  });
});

describe('mostUrgentTimelineRow', () => {
  it('returns undefined when every timeline row is ready', () => {
    const m = matrix([row('a', 'timeline', 'ready'), row('b', 'timeline', 'ready')]);
    expect(mostUrgentTimelineRow(m)).toBeUndefined();
  });

  it('ignores ready and notApplicable rows', () => {
    const m = matrix([row('a', 'timeline', 'ready'), row('b', 'timeline', 'notApplicable')]);
    expect(mostUrgentTimelineRow(m)).toBeUndefined();
  });

  it('ranks failed above deferred above running above partial', () => {
    const m = matrix([
      row('partial', 'timeline', 'partial'),
      row('running', 'timeline', 'running'),
      row('deferred', 'timeline', 'deferred'),
      row('failed', 'timeline', 'failed'),
    ]);
    expect(mostUrgentTimelineRow(m)?.dataSourceId).toBe('failed');

    expect(
      mostUrgentTimelineRow(matrix([row('a', 'timeline', 'running'), row('b', 'timeline', 'deferred')]))?.dataSourceId,
    ).toBe('b');
    expect(
      mostUrgentTimelineRow(matrix([row('a', 'timeline', 'partial'), row('b', 'timeline', 'running')]))?.dataSourceId,
    ).toBe('b');
  });

  it('treats queued like running', () => {
    const m = matrix([row('a', 'timeline', 'queued')]);
    expect(mostUrgentTimelineRow(m)?.state).toBe('queued');
  });
});
