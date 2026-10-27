import { useSyncExternalStore } from 'react';
import i18n from '@/i18n';
import { subscribeToEvent } from '@/lib/events/subscribers';
import type {
  DataSourceSummary,
  ImportPhase,
  ImportPhaseProgress,
  ImportPhaseState,
  IndexCacheStatus,
  JobCancellation,
  PartialResult,
  PartialResultKind,
  PerformanceReport,
  ResultFreshness,
} from '@/types/models';

export type EvidenceHashStatus = 'pending' | 'ready' | 'failed' | 'unavailable' | 'deferred';

export type ImportSignalSnapshot = {
  activeJobId?: string;
  latestPhase?: ImportPhaseProgress;
  latestCancellation?: JobCancellation;
  partialResults: PartialResult[];
  cacheStatuses: IndexCacheStatus[];
  latestReport?: PerformanceReport;
  lastUpdatedAt?: string;
};

type EventState = {
  activeJobId?: string;
  phases: Record<string, ImportPhaseProgress>;
  cancellations: Record<string, JobCancellation>;
  partialResults: Record<string, PartialResult>;
  caches: Record<string, IndexCacheStatus>;
  latestReport?: PerformanceReport;
  lastUpdatedAt?: string;
};

const freshnessPriority: Record<ResultFreshness, number> = {
  invalidated: 0,
  stale: 1,
  deferred: 2,
  partial: 3,
  ready: 4,
};

// Key paths, not literals: the label is resolved when it is rendered so a language
// change takes effect without reloading the module.
const phaseLabelKeys: Record<ImportPhase, string> = {
  queued: 'jobs.import.phase.queued',
  attach: 'jobs.import.phase.attach',
  probe: 'jobs.import.phase.probe',
  enumerate: 'jobs.import.phase.enumerate',
  mergeEnumeration: 'jobs.import.phase.mergeEnumeration',
  analyze: 'jobs.import.phase.analyze',
  mergeAnalysis: 'jobs.import.phase.mergeAnalysis',
  hashEvidence: 'jobs.import.phase.hashEvidence',
  buildIndexes: 'jobs.import.phase.buildIndexes',
  finalize: 'jobs.import.phase.finalize',
};

const phaseStateLabelKeys: Record<ImportPhaseState, string> = {
  pending: 'jobs.import.phaseState.pending',
  running: 'jobs.import.phaseState.running',
  completed: 'jobs.import.phaseState.completed',
  skipped: 'jobs.import.phaseState.skipped',
  cancelling: 'jobs.import.phaseState.cancelling',
  cancelled: 'jobs.import.phaseState.cancelled',
  failed: 'jobs.import.phaseState.failed',
  partial: 'jobs.import.phaseState.partial',
};

const partialKindLabelKeys: Record<PartialResultKind, string> = {
  fileTree: 'jobs.import.partialKind.fileTree',
  fileRows: 'jobs.import.partialKind.fileRows',
  partition: 'jobs.import.partialKind.partition',
  timelineEvents: 'jobs.import.partialKind.timelineEvents',
  timelineBuckets: 'jobs.import.partialKind.timelineBuckets',
  artifactFamily: 'jobs.import.partialKind.artifactFamily',
  searchIndex: 'jobs.import.partialKind.searchIndex',
  evidenceHash: 'jobs.import.partialKind.evidenceHash',
};

const freshnessLabelKeys: Record<ResultFreshness, string> = {
  ready: 'jobs.import.freshness.ready',
  partial: 'jobs.import.freshness.partial',
  deferred: 'jobs.import.freshness.deferred',
  stale: 'jobs.import.freshness.stale',
  invalidated: 'jobs.import.freshness.invalidated',
};

const evidenceHashStatusLabelKeys: Record<EvidenceHashStatus, string> = {
  pending: 'jobs.import.evidenceHashStatus.pending',
  ready: 'jobs.import.evidenceHashStatus.ready',
  failed: 'jobs.import.evidenceHashStatus.failed',
  unavailable: 'jobs.import.evidenceHashStatus.unavailable',
  deferred: 'jobs.import.evidenceHashStatus.deferred',
};

const evidenceHashStatusPriority: Record<EvidenceHashStatus, number> = {
  failed: 0,
  pending: 1,
  unavailable: 2,
  deferred: 3,
  ready: 4,
};

function createInitialState(): EventState {
  return {
    phases: {},
    cancellations: {},
    partialResults: {},
    caches: {},
  };
}

function partialKey(result: PartialResult) {
  return `${result.queryKey}::${result.scopeId}::${result.kind}`;
}

function sortPartialResults(results: PartialResult[]) {
  return [...results].sort((left, right) => {
    const freshnessDiff = freshnessPriority[left.freshness] - freshnessPriority[right.freshness];
    if (freshnessDiff !== 0) {
      return freshnessDiff;
    }

    if (left.readyCount !== right.readyCount) {
      return right.readyCount - left.readyCount;
    }

    return left.kind.localeCompare(right.kind);
  });
}

function sortCacheStatuses(statuses: IndexCacheStatus[]) {
  return [...statuses].sort((left, right) => right.updatedAt.localeCompare(left.updatedAt));
}

function deriveSnapshot(state: EventState): ImportSignalSnapshot {
  const activeJobId = state.activeJobId;

  return {
    activeJobId,
    latestPhase: activeJobId ? state.phases[activeJobId] : undefined,
    latestCancellation: activeJobId ? state.cancellations[activeJobId] : undefined,
    partialResults: sortPartialResults(Object.values(state.partialResults)),
    cacheStatuses: sortCacheStatuses(Object.values(state.caches)),
    latestReport: state.latestReport,
    lastUpdatedAt: state.lastUpdatedAt,
  };
}

export function createImportEventStateStore() {
  let state = createInitialState();
  let snapshot = deriveSnapshot(state);
  let connected = false;
  let unsubs: Array<() => void> = [];
  const listeners = new Set<() => void>();

  function emit() {
    snapshot = deriveSnapshot(state);
    listeners.forEach((listener) => listener());
  }

  function update(next: Partial<EventState>) {
    state = { ...state, ...next };
    emit();
  }

  function connect() {
    if (connected) {
      return;
    }

    connected = true;
    unsubs = [
      subscribeToEvent<ImportPhaseProgress>('import-phase-progress', (event) => {
        const payload = event.payload;
        const nextPartials = { ...state.partialResults };
        payload.partialResults.forEach((result) => {
          nextPartials[partialKey(result)] = result;
        });
        update({
          activeJobId: payload.jobId,
          phases: { ...state.phases, [payload.jobId]: payload },
          partialResults: nextPartials,
          lastUpdatedAt: event.ts,
        });
      }),
      subscribeToEvent<PartialResult>('import-partial-result', (event) => {
        const payload = event.payload;
        update({
          partialResults: { ...state.partialResults, [partialKey(payload)]: payload },
          lastUpdatedAt: event.ts,
        });
      }),
      subscribeToEvent<JobCancellation>('job-cancellation', (event) => {
        const payload = event.payload;
        update({
          activeJobId: payload.jobId,
          cancellations: { ...state.cancellations, [payload.jobId]: payload },
          lastUpdatedAt: event.ts,
        });
      }),
      subscribeToEvent<IndexCacheStatus>('cache-index-status', (event) => {
        const payload = event.payload;
        update({
          caches: { ...state.caches, [payload.cacheKey]: payload },
          lastUpdatedAt: event.ts,
        });
      }),
      subscribeToEvent<PerformanceReport>('performance-report-ready', (event) => {
        const payload = event.payload;
        update({
          activeJobId: payload.summary.jobId ?? state.activeJobId,
          latestReport: payload,
          lastUpdatedAt: event.ts,
        });
      }),
    ];
  }

  return {
    subscribe(listener: () => void) {
      connect();
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    getSnapshot() {
      return snapshot;
    },
    ingestPhase(eventTs: string, payload: ImportPhaseProgress) {
      const nextPartials = { ...state.partialResults };
      payload.partialResults.forEach((result) => {
        nextPartials[partialKey(result)] = result;
      });
      update({
        activeJobId: payload.jobId,
        phases: { ...state.phases, [payload.jobId]: payload },
        partialResults: nextPartials,
        lastUpdatedAt: eventTs,
      });
    },
    ingestPartial(eventTs: string, payload: PartialResult) {
      update({
        partialResults: { ...state.partialResults, [partialKey(payload)]: payload },
        lastUpdatedAt: eventTs,
      });
    },
    ingestCancellation(eventTs: string, payload: JobCancellation) {
      update({
        activeJobId: payload.jobId,
        cancellations: { ...state.cancellations, [payload.jobId]: payload },
        lastUpdatedAt: eventTs,
      });
    },
    ingestCache(eventTs: string, payload: IndexCacheStatus) {
      update({
        caches: { ...state.caches, [payload.cacheKey]: payload },
        lastUpdatedAt: eventTs,
      });
    },
    ingestReport(eventTs: string, payload: PerformanceReport) {
      update({
        activeJobId: payload.summary.jobId ?? state.activeJobId,
        latestReport: payload,
        lastUpdatedAt: eventTs,
      });
    },
    reset() {
      state = createInitialState();
      snapshot = deriveSnapshot(state);
      emit();
    },
    disconnect() {
      unsubs.forEach((unsub) => unsub());
      unsubs = [];
      connected = false;
    },
  };
}

const importEventStateStore = createImportEventStateStore();

export function useImportEventState() {
  return useSyncExternalStore(importEventStateStore.subscribe, importEventStateStore.getSnapshot, importEventStateStore.getSnapshot);
}

export function resetImportEventStateForTests() {
  importEventStateStore.disconnect();
  importEventStateStore.reset();
}

export function getImportPhaseLabel(phase?: ImportPhase) {
  return i18n.t(phase ? phaseLabelKeys[phase] : 'jobs.import.phase.idle');
}

export function getImportPhaseStateLabel(state?: ImportPhaseState) {
  return i18n.t(state ? phaseStateLabelKeys[state] : 'jobs.import.phaseState.waiting');
}

export function getPartialKindLabel(kind: PartialResultKind) {
  return i18n.t(partialKindLabelKeys[kind]);
}

export function getFreshnessLabel(freshness: ResultFreshness) {
  return i18n.t(freshnessLabelKeys[freshness]);
}

export function getEvidenceHashStatusLabel(status: EvidenceHashStatus) {
  return i18n.t(evidenceHashStatusLabelKeys[status]);
}

export function getEvidenceHashCaveatText(status: EvidenceHashStatus) {
  switch (status) {
    case 'ready':
      return 'Evidence hash status is ready for included sources.';
    case 'pending':
      return 'Evidence hash work is still pending; report verification is not complete yet.';
    case 'failed':
      return 'Evidence hash work failed for at least one source; verify evidence completeness manually.';
    case 'unavailable':
      return 'Evidence hash is unavailable for at least one source; report completeness has a hash caveat.';
    case 'deferred':
      return 'Evidence hash work is deferred or stale; report verification is intentionally caveated.';
  }
}

export function deriveEvidenceHashStatus(
  partialResults: PartialResult[],
  dataSources: DataSourceSummary[] = [],
): EvidenceHashStatus | undefined {
  const statuses: EvidenceHashStatus[] = [];

  for (const result of partialResults) {
    if (result.kind !== 'evidenceHash') {
      continue;
    }

    if (result.freshness === 'ready') {
      statuses.push('ready');
    } else if (result.freshness === 'partial') {
      statuses.push('pending');
    } else {
      statuses.push('deferred');
    }
  }

  for (const source of dataSources) {
    const normalized = (source.originalDiskHashStatus ?? source.hashStatus)?.toLowerCase();
    if (normalized === 'hashed' || normalized === 'recorded' || normalized === 'ready') {
      statuses.push('ready');
    } else if (normalized === 'pending') {
      statuses.push('pending');
    } else if (normalized === 'failed') {
      statuses.push('failed');
    } else if (normalized === 'unavailable') {
      statuses.push('unavailable');
    } else if (normalized === 'unknown' || normalized === 'deferred') {
      statuses.push('deferred');
    }
  }

  return statuses.sort((left, right) => evidenceHashStatusPriority[left] - evidenceHashStatusPriority[right])[0];
}

export function getCacheStateLabel(state: string) {
  const labels: Record<string, string> = {
    ready: 'Ready',
    partial: 'Partial',
    deferred: 'Deferred',
    stale: 'Stale',
    invalidated: 'Invalidated',
    warming: 'Warming',
    reused: 'Reused',
    pending: 'Pending',
    failed: 'Failed',
    unavailable: 'Unavailable',
    cancelled: 'Cancelled',
    draining: 'Draining',
  };

  return labels[state] ?? state;
}
