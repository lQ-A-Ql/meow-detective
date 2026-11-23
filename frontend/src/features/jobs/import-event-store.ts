import { createStore } from 'zustand/vanilla';
import { subscribeToEvent } from '@/lib/events/subscribers';
import type {
  ImportPhaseProgress,
  IndexCacheStatus,
  JobCancellation,
  PartialResult,
  PerformanceReport,
} from '@/types/models';

export interface ImportSignalSnapshot {
  caseId?: string;
  activeJobId?: string;
  latestPhase?: ImportPhaseProgress;
  phasesByJob: Record<string, ImportPhaseProgress>;
  cancellationsByJob: Record<string, JobCancellation>;
  latestCancellation?: JobCancellation;
  partialResults: PartialResult[];
  cacheStatuses: IndexCacheStatus[];
  latestReport?: PerformanceReport;
  lastUpdatedAt?: string;
}

export const emptyImportSignals: ImportSignalSnapshot = {
  phasesByJob: {},
  cancellationsByJob: {},
  partialResults: [],
  cacheStatuses: [],
};

const MAX_JOB_SIGNALS = 128;
const freshnessPriority = { invalidated: 0, stale: 1, deferred: 2, partial: 3, ready: 4 };

function mergePartials(existing: PartialResult[], incoming: PartialResult[]) {
  const results = new Map(
    existing.map((result) => [JSON.stringify([result.queryKey, result.scopeId, result.kind]), result]),
  );
  incoming.forEach((result) => results.set(JSON.stringify([result.queryKey, result.scopeId, result.kind]), result));
  return Array.from(results.values()).sort(
    (left, right) =>
      freshnessPriority[left.freshness] - freshnessPriority[right.freshness] ||
      right.readyCount - left.readyCount ||
      left.kind.localeCompare(right.kind),
  );
}

export function createImportEventStateStore(initialCaseId?: string) {
  const api = createStore<ImportSignalSnapshot>(() => ({ ...emptyImportSignals, caseId: initialCaseId }));
  const phaseTimes = new Map<string, number>();
  const cancellationTimes = new Map<string, number>();
  let unsubs: Array<() => void> = [];

  function ingestPartial(eventTs: string, payload: PartialResult) {
    api.setState((state) => ({
      partialResults: mergePartials(state.partialResults, [payload]),
      lastUpdatedAt: eventTs,
    }));
  }

  function ingestPhase(eventTs: string, payload: ImportPhaseProgress) {
    const state = api.getState();
    const timestamp = Date.parse(eventTs);
    if (state.caseId && payload.caseId !== state.caseId) return;
    if (!Number.isFinite(timestamp) || timestamp < (phaseTimes.get(payload.jobId) ?? -Infinity)) return;
    phaseTimes.delete(payload.jobId);
    phaseTimes.set(payload.jobId, timestamp);
    const previous = state.phasesByJob[payload.jobId];
    const phase = { ...payload, percent: Math.max(previous?.percent ?? 0, payload.percent) };
    const phasesByJob = { ...state.phasesByJob, [payload.jobId]: phase };
    const cancellationsByJob = { ...state.cancellationsByJob };
    if (phaseTimes.size > MAX_JOB_SIGNALS) {
      const oldest = phaseTimes.keys().next().value!;
      phaseTimes.delete(oldest);
      cancellationTimes.delete(oldest);
      delete phasesByJob[oldest];
      delete cancellationsByJob[oldest];
    }
    api.setState({
      activeJobId: payload.jobId,
      latestPhase: phase,
      latestCancellation: cancellationsByJob[payload.jobId],
      phasesByJob,
      cancellationsByJob,
      partialResults: mergePartials(state.partialResults, payload.partialResults),
      lastUpdatedAt: eventTs,
    });
  }

  function ingestCancellation(eventTs: string, payload: JobCancellation) {
    const state = api.getState();
    const timestamp = Date.parse(eventTs);
    if (state.caseId && !state.phasesByJob[payload.jobId]) return;
    if (!Number.isFinite(timestamp) || timestamp < (cancellationTimes.get(payload.jobId) ?? -Infinity)) return;
    cancellationTimes.set(payload.jobId, timestamp);
    api.setState({
      activeJobId: payload.jobId,
      latestPhase: state.phasesByJob[payload.jobId],
      latestCancellation: payload,
      cancellationsByJob: { ...state.cancellationsByJob, [payload.jobId]: payload },
      lastUpdatedAt: eventTs,
    });
  }

  function ingestCache(eventTs: string, payload: IndexCacheStatus) {
    api.setState((state) => ({
      cacheStatuses: [...state.cacheStatuses.filter((status) => status.cacheKey !== payload.cacheKey), payload].sort(
        (left, right) => right.updatedAt.localeCompare(left.updatedAt),
      ),
      lastUpdatedAt: eventTs,
    }));
  }

  function ingestReport(eventTs: string, payload: PerformanceReport) {
    api.setState({ latestReport: payload, lastUpdatedAt: eventTs });
  }

  function reset(caseId?: string) {
    phaseTimes.clear();
    cancellationTimes.clear();
    api.setState({ ...emptyImportSignals, caseId }, true);
  }

  function connect() {
    if (unsubs.length) return;
    unsubs = [
      subscribeToEvent<ImportPhaseProgress>('import-phase-progress', (event) => ingestPhase(event.ts, event.payload)),
      subscribeToEvent<PartialResult>('import-partial-result', (event) => ingestPartial(event.ts, event.payload)),
      subscribeToEvent<JobCancellation>('job-cancellation', (event) => ingestCancellation(event.ts, event.payload)),
      subscribeToEvent<IndexCacheStatus>('cache-index-status', (event) => ingestCache(event.ts, event.payload)),
      subscribeToEvent<PerformanceReport>('performance-report-ready', (event) => ingestReport(event.ts, event.payload)),
      subscribeToEvent('case-closed', () => reset(api.getState().caseId)),
    ];
  }

  return {
    api,
    connect,
    getSnapshot: api.getState,
    ingestPhase,
    ingestPartial,
    ingestCancellation,
    ingestCache,
    ingestReport,
    reset,
    setCase(caseId?: string) {
      if (caseId !== api.getState().caseId) reset(caseId);
    },
    disconnect() {
      unsubs.forEach((unsubscribe) => unsubscribe());
      unsubs = [];
    },
  };
}
