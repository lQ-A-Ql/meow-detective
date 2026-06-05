import { describe, expect, it } from 'vitest';
import { createImportEventStateStore, deriveEvidenceHashStatus } from './import-event-state';

describe('import event state store', () => {
  it('merges typed progress, partial freshness, cancellation, cache, and performance report state', () => {
    const store = createImportEventStateStore();

    store.ingestPhase('2026-06-05T10:00:00Z', {
      jobId: 'job-42',
      caseId: 'case-7',
      dataSourceId: 'ds-9',
      phase: 'analyze',
      state: 'running',
      percent: 64,
      detail: 'scheduling=draining workerBudget=4',
      metrics: {
        elapsedMs: 1200,
        rssMb: 256,
        workers: 2,
        rowsProcessed: 640,
        rowsTotal: 1000,
        rowsPerSec: 50,
        bytesProcessed: 1024,
        bytesTotal: 4096,
        mbPerSec: 2,
        warnings: 1,
        skipped: 0,
        failed: 0,
      },
      partialResults: [
        {
          kind: 'searchIndex',
          scopeId: 'ds-9',
          readyCount: 120,
          totalEstimate: 400,
          queryKey: 'search:index:ds-9',
          freshness: 'partial',
        },
      ],
      cancellable: true,
      cancelRequested: true,
    });

    store.ingestPartial('2026-06-05T10:01:00Z', {
      kind: 'timelineEvents',
      scopeId: 'ds-9',
      readyCount: 800,
      totalEstimate: 1200,
      queryKey: 'timeline:events:ds-9',
      freshness: 'stale',
    });

    store.ingestCancellation('2026-06-05T10:02:00Z', {
      jobId: 'job-42',
      state: 'draining',
      safeToClose: false,
      detail: 'Waiting for workers to settle',
      requestedAt: '2026-06-05T10:01:30Z',
      acknowledgedAt: '2026-06-05T10:01:40Z',
    });

    store.ingestCache('2026-06-05T10:03:00Z', {
      cacheKey: 'search:index:ds-9',
      state: 'warming',
      indexedCount: 300,
      totalCount: 1000,
      updatedAt: '2026-06-05T10:03:00Z',
      message: 'Index warming',
    });

    store.ingestReport('2026-06-05T10:04:00Z', {
      summary: {
        reportId: 'perf-1',
        jobId: 'job-42',
        generatedAt: '2026-06-05T10:04:00Z',
        elapsedMs: 842,
        peakMemoryBytes: 1048576,
        summary: 'Timeline query stayed within bounded metrics.',
      },
      metrics: [
        {
          key: 'timeline.query.elapsedMs',
          value: 842,
          unit: 'ms',
        },
      ],
    });

    const snapshot = store.getSnapshot();

    expect(snapshot.activeJobId).toBe('job-42');
    expect(snapshot.latestPhase?.phase).toBe('analyze');
    expect(snapshot.latestCancellation?.state).toBe('draining');
    expect(snapshot.latestCancellation?.safeToClose).toBe(false);
    expect(snapshot.partialResults.map((result) => result.freshness)).toEqual(['stale', 'partial']);
    expect(snapshot.cacheStatuses[0]).toEqual(expect.objectContaining({ cacheKey: 'search:index:ds-9', state: 'warming' }));
    expect(snapshot.latestReport).toEqual(expect.objectContaining({
      summary: expect.objectContaining({ reportId: 'perf-1', elapsedMs: 842 }),
      metrics: [expect.objectContaining({ key: 'timeline.query.elapsedMs', value: 842, unit: 'ms' })],
    }));
  });

  it('derives evidence hash status from partial freshness and data source hash status', () => {
    expect(deriveEvidenceHashStatus([
      {
        kind: 'evidenceHash',
        scopeId: 'ds-ready',
        readyCount: 1,
        totalEstimate: 1,
        queryKey: 'evidence:hash:ds-ready',
        freshness: 'ready',
      },
    ])).toBe('ready');

    expect(deriveEvidenceHashStatus([
      {
        kind: 'evidenceHash',
        scopeId: 'ds-pending',
        readyCount: 0,
        totalEstimate: 1,
        queryKey: 'evidence:hash:ds-pending',
        freshness: 'partial',
      },
    ])).toBe('pending');

    expect(deriveEvidenceHashStatus([], [
      {
        id: 'ds-unavailable',
        name: 'Logical Source',
        kind: 'logical_directory',
        sourcePath: 'redacted',
        importedAt: '2026-06-05T10:00:00Z',
        hashStatus: 'unavailable',
        partitions: [],
      },
    ])).toBe('unavailable');

    expect(deriveEvidenceHashStatus([
      {
        kind: 'evidenceHash',
        scopeId: 'ds-ready',
        readyCount: 1,
        totalEstimate: 1,
        queryKey: 'evidence:hash:ds-ready',
        freshness: 'ready',
      },
    ], [
      {
        id: 'ds-failed',
        name: 'Failed Source',
        kind: 'raw',
        sourcePath: 'redacted',
        importedAt: '2026-06-05T10:00:00Z',
        hashStatus: 'failed',
        partitions: [],
      },
    ])).toBe('failed');
  });
});
