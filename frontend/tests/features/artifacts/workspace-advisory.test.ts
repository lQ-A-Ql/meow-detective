import { describe, expect, it } from 'vitest';
import { pickArtifactsCapabilityAdvisory } from '@/features/artifacts/use-artifacts-workspace-model';
import type { SourceCapability } from '@/types/models';

function row(state: SourceCapability['state'], extra: Partial<SourceCapability> = {}): SourceCapability {
  return {
    dataSourceId: 'source-1',
    capability: 'artifacts',
    state,
    revision: 1,
    derived: false,
    updatedAt: '2026-11-20T00:00:00Z',
    ...extra,
  };
}

describe('pickArtifactsCapabilityAdvisory', () => {
  it('surfaces a partial row instead of treating it as complete', () => {
    const advisory = pickArtifactsCapabilityAdvisory([row('partial', { detail: 'retryableFailures=2 warnings=1' })]);
    expect(advisory?.state).toBe('partial');
    expect(advisory?.detail).toContain('retryableFailures=2');
  });

  it('ignores a ready row so complete results show no advisory', () => {
    expect(pickArtifactsCapabilityAdvisory([row('ready')])).toBeUndefined();
  });

  it('prefers running/queued over partial and deferred', () => {
    expect(pickArtifactsCapabilityAdvisory([row('deferred'), row('partial'), row('running')])?.state).toBe('running');
    expect(pickArtifactsCapabilityAdvisory([row('deferred'), row('partial'), row('queued')])?.state).toBe('queued');
  });

  it('prefers partial over deferred', () => {
    expect(pickArtifactsCapabilityAdvisory([row('deferred'), row('partial')])?.state).toBe('partial');
  });

  it('only considers the artifacts capability kind', () => {
    expect(
      pickArtifactsCapabilityAdvisory([{ ...row('partial'), capability: 'timeline' }, row('ready')]),
    ).toBeUndefined();
  });

  it('returns undefined when the matrix has no artifacts row', () => {
    expect(pickArtifactsCapabilityAdvisory([])).toBeUndefined();
  });
});
