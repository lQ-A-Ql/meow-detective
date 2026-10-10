import { describe, expect, expectTypeOf, it } from 'vitest';
import type {
  EventTopic,
  ImportDataSourceRequest,
  ImportPhaseProgress,
  ImportTargetPlatform,
  IndexCacheStatus,
  JobCancellation,
  PartialResult,
  PerformanceReport,
  PerformanceReportSummary,
} from '@/types/models';

describe('import progress contract models', () => {
  it('requires an explicit supported platform for data-source imports', () => {
    expectTypeOf<ImportTargetPlatform>().toEqualTypeOf<'windows' | 'linux' | 'android'>();
    expectTypeOf<ImportDataSourceRequest['platform']>().toEqualTypeOf<
      'windows' | 'linux' | 'android'
    >();

    const request = {
      sourcePath: 'D:/evidence/windows.E01',
      platform: 'windows',
    } satisfies ImportDataSourceRequest;

    expect(request.platform).toBe('windows');
  });

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

    // The declarations above are the contract (checked by tsc); assert the wire shape.
    expect(new Set(phases).size).toBe(phases.length);
    expect(phases.every((phase) => /^[a-z][A-Za-z0-9]*$/.test(phase))).toBe(true);
    expect(new Set(partials).size).toBe(partials.length);
    expect(new Set(freshness).size).toBe(freshness.length);
  });

  it('accepts cancellation payloads with detailed design states', () => {
    const cancellation = {
      jobId: 'job-1',
      requestedAt: '2026-06-05T00:02:00Z',
      acknowledgedAt: '2026-06-05T00:02:01Z',
      state: 'draining',
      safeToClose: false,
      detail: 'Draining import workers',
    } satisfies JobCancellation;
    const states: JobCancellation['state'][] = ['notRequested', 'requested', 'acknowledged', 'draining', 'cancelled', 'timedOut'];

    expect(cancellation.state).toBe('draining');
    expect(cancellation.safeToClose).toBe(false);
    expect(new Set(states).size).toBe(states.length);
    expect(states.every((state) => /^[a-z][A-Za-z0-9]*$/.test(state))).toBe(true);
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

    expect(cache.cacheKey).toContain(':');
    expect(cache.indexedCount).toBeLessThanOrEqual(cache.totalCount);
    expect(report.summary.elapsedMs).toBeGreaterThan(0);
    expect(report.metrics.every((metric) => /^[a-z][A-Za-z0-9.]*$/.test(metric.key))).toBe(true);
  });

  it('includes Tauri-safe import, cache, cancellation, and performance event topics', () => {
    const topics = [
      'import-phase-progress',
      'import-partial-result',
      'job-cancellation',
      'cache-index-status',
      'performance-report-ready',
    ] satisfies EventTopic[];

    // Topics are emitted as kebab-case wire strings with no dotted namespace prefix.
    expect(new Set(topics).size).toBe(topics.length);
    expect(topics.every((topic) => !topic.includes('.'))).toBe(true);
    expect(topics.every((topic) => /^[A-Za-z0-9_:/-]+$/.test(topic))).toBe(true);
  });
});
