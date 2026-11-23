import { useEffect } from 'react';
import { useStore } from 'zustand';
import i18n from '@/i18n';
import { createImportEventStateStore, emptyImportSignals } from './import-event-store';
import type {
  DataSourceSummary,
  ImportPhase,
  ImportPhaseState,
  PartialResult,
  PartialResultKind,
  ResultFreshness,
} from '@/types/models';

export { createImportEventStateStore } from './import-event-store';
export type { ImportSignalSnapshot } from './import-event-store';
export type EvidenceHashStatus = 'pending' | 'ready' | 'failed' | 'unavailable' | 'deferred';

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
  timeline: 'jobs.import.phase.timeline',
  checkpoint: 'jobs.import.phase.checkpoint',
  postReady: 'jobs.import.phase.postReady',
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

const evidenceHashStatusPriority: Record<EvidenceHashStatus, number> = {
  failed: 0,
  pending: 1,
  unavailable: 2,
  deferred: 3,
  ready: 4,
};

const importEventStateStore = createImportEventStateStore();

export function useImportEventState(caseId?: string) {
  useEffect(() => {
    importEventStateStore.setCase(caseId);
    importEventStateStore.connect();
  }, [caseId]);
  return useStore(importEventStateStore.api, (state) => (state.caseId === caseId ? state : emptyImportSignals));
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
  return i18n.t(`jobs.import.evidenceHashStatus.${status}`);
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
    const normalized = source.hashStatus?.toLowerCase();
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
