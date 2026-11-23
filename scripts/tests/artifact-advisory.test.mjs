import assert from 'node:assert/strict';
import test from 'node:test';
import {
  artifactAdvisoryPresentation,
  pickArtifactsCapabilityAdvisory,
} from '../../frontend/src/features/artifacts/capability-advisory.ts';

function row(state) {
  return { dataSourceId: 'source-a', capability: 'artifacts', state, revision: 1, derived: false, updatedAt: '2026-11-23T00:00:00Z' };
}

test('failed and cancelled extraction remain visible among other active sources', () => {
  assert.equal(pickArtifactsCapabilityAdvisory([row('running'), row('failed')]).state, 'failed');
  assert.equal(pickArtifactsCapabilityAdvisory([row('running'), row('cancelled')]).state, 'cancelled');
});

test('failed, cancelled, partial and deferred states offer the real retry action', () => {
  for (const state of ['failed', 'cancelled', 'partial', 'deferred']) {
    const presentation = artifactAdvisoryPresentation(row(state));
    assert.equal(presentation.messageKey, `artifacts.capability.${state}`);
    assert.equal(presentation.retryable, true);
    assert.equal(presentation.active, false);
  }
});

test('active extraction cannot be retried while completed and inapplicable rows stay absent', () => {
  for (const state of ['running', 'queued']) {
    const presentation = artifactAdvisoryPresentation(row(state));
    assert.equal(presentation.retryable, false);
    assert.equal(presentation.active, true);
  }
  assert.equal(pickArtifactsCapabilityAdvisory([row('ready'), row('notApplicable')]), undefined);
  assert.equal(pickArtifactsCapabilityAdvisory([{ ...row('failed'), capability: 'timeline' }]), undefined);
});
