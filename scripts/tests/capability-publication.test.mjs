import assert from 'node:assert/strict';
import { createRequire } from 'node:module';
import test from 'node:test';
import { invalidateEventProjectionQueries } from '../../frontend/src/features/cache-invalidation.ts';

const requireFrontend = createRequire(new URL('../../frontend/package.json', import.meta.url));
const { QueryClient } = requireFrontend('@tanstack/react-query');

function cache() {
  const client = new QueryClient();
  const keys = [
    ['capability', 'matrix', 'case-a'],
    ['capability', 'matrix', 'case-b'],
    ['artifacts', 'case-a', 'counts'],
    ['artifacts', 'case-b', 'counts'],
    ['timeline', 'case-a', 'events'],
    ['timeline', 'case-b', 'events'],
    ['graph', 'snapshot', 'case-a'],
    ['graph', 'snapshot', 'case-b'],
    ['analysis', 'browser-history', 'case-a', 'source-a'],
    ['analysis', 'browser-history', 'case-a', 'source-b'],
    ['analysis', 'case-overview', 'case-a'],
    ['case', 'metrics', 'case-a'],
  ];
  for (const key of keys) client.setQueryData(key, []);
  return client;
}

function stale(client, key) {
  return client.getQueryState(key).isInvalidated;
}

test('artifact publication refreshes its case and source together with aggregate projections', () => {
  const client = cache();
  invalidateEventProjectionQueries(client, 'capability-status-changed', {
    caseId: 'case-a', dataSourceId: 'source-a', capability: 'artifacts', state: 'ready', revision: 3,
  });
  assert.equal(stale(client, ['artifacts', 'case-a', 'counts']), true);
  assert.equal(stale(client, ['timeline', 'case-a', 'events']), true);
  assert.equal(stale(client, ['graph', 'snapshot', 'case-a']), true);
  assert.equal(stale(client, ['analysis', 'browser-history', 'case-a', 'source-a']), true);
  assert.equal(stale(client, ['analysis', 'case-overview', 'case-a']), true);
  assert.equal(stale(client, ['case', 'metrics', 'case-a']), true);
  assert.equal(stale(client, ['analysis', 'browser-history', 'case-a', 'source-b']), false);
  assert.equal(stale(client, ['artifacts', 'case-b', 'counts']), false);
  assert.equal(stale(client, ['timeline', 'case-b', 'events']), false);
  assert.equal(stale(client, ['graph', 'snapshot', 'case-b']), false);
  assert.equal(stale(client, ['capability', 'matrix', 'case-b']), false);
  client.clear();
});

test('queued and running events refresh the matrix without refetching result families', () => {
  for (const state of ['queued', 'running']) {
    const client = cache();
    invalidateEventProjectionQueries(client, 'capability-status-changed', {
      caseId: 'case-a', capability: 'artifacts', state,
    });
    assert.equal(stale(client, ['capability', 'matrix', 'case-a']), true);
    assert.equal(stale(client, ['artifacts', 'case-a', 'counts']), false);
    client.clear();
  }
});

test('manual timeline publication refreshes the matrix and published graph in the same case', () => {
  const client = cache();
  invalidateEventProjectionQueries(client, 'timeline-updated', { caseId: 'case-a' });
  assert.equal(stale(client, ['capability', 'matrix', 'case-a']), true);
  assert.equal(stale(client, ['timeline', 'case-a', 'events']), true);
  assert.equal(stale(client, ['graph', 'snapshot', 'case-a']), true);
  assert.equal(stale(client, ['capability', 'matrix', 'case-b']), false);
  client.clear();
});

test('terminal partial, failed and cancelled results also refresh previously published artifact data', () => {
  for (const state of ['partial', 'failed', 'cancelled']) {
    const client = cache();
    invalidateEventProjectionQueries(client, 'capability-status-changed', {
      caseId: 'case-a', capability: 'artifacts', state,
    });
    assert.equal(stale(client, ['artifacts', 'case-a', 'counts']), true);
    client.clear();
  }
});
