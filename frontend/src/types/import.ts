export type ImportPhase =
  | 'queued'
  | 'attach'
  | 'probe'
  | 'enumerate'
  | 'mergeEnumeration'
  | 'analyze'
  | 'mergeAnalysis'
  | 'hashEvidence'
  | 'buildIndexes'
  | 'finalize';

export type ImportPhaseState =
  | 'pending'
  | 'running'
  | 'completed'
  | 'skipped'
  | 'cancelling'
  | 'cancelled'
  | 'failed'
  | 'partial';

export interface ImportPhaseMetrics {
  elapsedMs: number;
  rssMb: number;
  workers: number;
  rowsProcessed: number;
  rowsTotal?: number;
  rowsPerSec?: number;
  bytesProcessed: number;
  bytesTotal?: number;
  mbPerSec?: number;
  warnings: number;
  skipped: number;
  failed: number;
}

export interface ImportPhaseProgress {
  jobId: string;
  caseId: string;
  dataSourceId?: string;
  phase: ImportPhase;
  state: ImportPhaseState;
  percent: number;
  detail: string;
  metrics: ImportPhaseMetrics;
  partialResults: PartialResult[];
  cancellable: boolean;
  cancelRequested: boolean;
}

export type PartialResultKind =
  | 'fileTree'
  | 'fileRows'
  | 'partition'
  | 'timelineEvents'
  | 'timelineBuckets'
  | 'artifactFamily'
  | 'searchIndex'
  | 'evidenceHash';

export type ResultFreshness = 'ready' | 'partial' | 'deferred' | 'stale' | 'invalidated';

export interface PartialResult {
  kind: PartialResultKind;
  scopeId: string;
  readyCount: number;
  totalEstimate?: number;
  queryKey: string;
  freshness: ResultFreshness;
}

export type CancelReason = 'userRequested' | 'caseClosing' | 'memoryLimit' | 'superseded';

export interface CancelJobRequest {
  jobId: string;
  reason: CancelReason;
  drainTimeoutMs: number;
}

export type CancellationState = 'notRequested' | 'requested' | 'acknowledged' | 'draining' | 'cancelled' | 'timedOut';

export interface JobCancellation {
  jobId: string;
  requestedAt?: string;
  acknowledgedAt?: string;
  state: CancellationState;
  safeToClose: boolean;
  detail: string;
}

export interface IndexCacheStatus {
  cacheKey: string;
  state: string;
  indexedCount: number;
  totalCount?: number;
  updatedAt: string;
  message?: string;
}

export interface PerformanceReportSummary {
  reportId: string;
  jobId?: string;
  generatedAt: string;
  elapsedMs: number;
  peakMemoryBytes?: number;
  summary: string;
}

export interface PerformanceMetric {
  key: string;
  value: number;
  unit: string;
}

export interface PerformanceReport {
  summary: PerformanceReportSummary;
  metrics: PerformanceMetric[];
}

export interface WarningItem {
  id: string;
  title: string;
  detail: string;
}

export interface TraceItem {
  id: string;
  ts: string;
  message: string;
}
