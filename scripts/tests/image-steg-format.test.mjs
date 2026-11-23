import assert from 'node:assert/strict';
import test from 'node:test';
import { formatStegRatio, formatStegScore } from '../../frontend/src/features/files/image-steg-format.ts';

test('suspicion scores retain the transport 0..100 point scale', () => {
  assert.equal(formatStegScore(0), '0.0 / 100');
  assert.equal(formatStegScore(30), '30.0 / 100');
  assert.equal(formatStegScore(82.25), '82.3 / 100');
  assert.equal(formatStegScore(100), '100.0 / 100');
});

test('detector ratios use percentages on the transport 0..1 scale', () => {
  assert.equal(formatStegRatio(0), '0.0%');
  assert.equal(formatStegRatio(0.3), '30.0%');
  assert.equal(formatStegRatio(1), '100.0%');
});

test('omitted and non-finite values keep their unavailable presentation', () => {
  for (const value of [undefined, Number.NaN, Number.POSITIVE_INFINITY]) {
    assert.equal(formatStegRatio(value), '-');
    assert.equal(formatStegScore(value), '-');
  }
});
