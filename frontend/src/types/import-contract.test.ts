import { describe, expect, it } from 'vitest';
import type {
  CancelJobRequest,
  EventTopic,
  ImportPhaseProgress,
  IndexCacheStatus,
  JobCancellation,
  PartialResult,
  PerformanceReport,
  PerformanceReportSummary,
} from './models';

describe('import progress contract models', () => {
  it('accepts detailed design import progress payloads', () => {
    const partial = {
      kind: 'timelineBuckets',
      scopeId: 'case-1',
      readyCount: 6,
      totalEstimate: 12,
      queryKey: 'timeline:buckets:case-1',
      freshness: 'partial',
    } satisfies PartialResult;
    const payload = {
      jobId: 'job-1',
      caseId: 'case-1',
      dataSourceId: 'ds-1',
      phase: 'mergeAnalysis',
      state: 'partial',
      percent: 42,
      detail: 'Merging worker output',
      metrics: {
        elapsedMs: 250,
        rssMb: 512,
        workers: 4,
        rowsProcessed: 10,
        rowsTotal: 20,
        rowsPerSec: 40,
        bytesProcessed: 1024,
        bytesTotal: 2048,
        mbPerSec: 8.5,
        warnings: 1,
        skipped: 2,
        failed: 0,
      },
      partialResults: [partial],
      cancellable: true,
      cancelRequested: false,
    } satisfies ImportPhaseProgress;

    expect(payload.phase).toBe('mergeAnalysis');
    expect(payload.state).toBe('partial');
    expect(payload.percent).toBe(42);
    expect(payload.metrics.rssMb).toBe(512);
    expect(payload.partialResults[0].kind).toBe('timelineBuckets');
    expect(payload.cancelRequested).toBe(false);
  });

  it('accepts all import phase and partial result design enum literals', () => {
    const phases: ImportPhaseProgress['phase'][] = [
      'queued',
      'attach',
      'probe',
      'enumerate',
      'mergeEnumeration',
      'analyze',
      'mergeAnalysis',
      'hashEvidence',
      'buildIndexes',
      'finalize',
    ];
    const partials: PartialResult['kind'][] = [
      'fileTree',
      'fileRows',
      'partition',
      'timelineEvents',
      'timelineBuckets',
      'artifactFamily',
      'searchIndex',
      'evidenceHash',
    ];
    const freshness: PartialResult['freshness'][] = ['ready', 'partial', 'deferred', 'stale', 'invalidated'];

    expect(phases).toContain('buildIndexes');
    expect(partials).toContain('artifactFamily');
    expect(freshness).toContain('invalidated');
  });

  it('accepts cancellation payloads with detailed design states', () => {
    const request = {
      jobId: 'job-1',
      reason: 'memoryLimit',
      drainTimeoutMs: 30_000,
    } satisfies CancelJobRequest;
    const cancellation = {
      jobId: 'job-1',
      requestedAt: '2026-06-05T00:02:00Z',
      acknowledgedAt: '2026-06-05T00:02:01Z',
      state: 'draining',
      safeToClose: false,
      detail: 'Draining import workers',
    } satisfies JobCancellation;
    const states: JobCancellation['state'][] = ['notRequested', 'requested', 'acknowledged', 'draining', 'cancelled', 'timedOut'];
    const reasons: CancelJobRequest['reason'][] = ['userRequested', 'caseClosing', 'memoryLimit', 'superseded'];

    expect(request.reason).toBe('memoryLimit');
    expect(request.drainTimeoutMs).toBe(30_000);
    expect(cancellation.state).toBe('draining');
    expect(cancellation.safeToClose).toBe(false);
    expect(states).toContain('timedOut');
    expect(reasons).toContain('caseClosing');
  });

  it('accepts cache and performance report payloads', () => {
    const cache = {
      cacheKey: 'case:files',
      state: 'warming',
      indexedCount: 12,
      totalCount: 20,
      updatedAt: '2026-06-05T00:03:00Z',
    } satisfies IndexCacheStatus;
    const reportSummary = {
      reportId: 'perf-1',
      jobId: 'job-1',
      generatedAt: '2026-06-05T00:04:00Z',
      elapsedMs: 3000,
      peakMemoryBytes: 65536,
      summary: 'Import completed within budget',
    } satisfies PerformanceReportSummary;
    const report = {
      summary: reportSummary,
      metrics: [
        {
          key: 'timeline.query.elapsedMs',
          value: 3000,
          unit: 'ms',
        },
      ],
    } satisfies PerformanceReport;

    expect(cache.cacheKey).toBe('case:files');
    expect(report.summary.peakMemoryBytes).toBe(65536);
    expect(report.metrics[0].key).toBe('timeline.query.elapsedMs');
  });

  it('includes dotted import, cache, cancellation, and performance event topics', () => {
    const topics = [
      'import.phase_progress',
      'import.partial_result',
      'job.cancellation',
      'cache.index_status',
      'performance.report_ready',
    ] satisfies EventTopic[];

    expect(topics).toContain('import.phase_progress');
    expect(topics).toContain('performance.report_ready');
  });
});
