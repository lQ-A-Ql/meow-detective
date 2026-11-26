import type { QueryClient, QueryFilters } from '@tanstack/react-query';
import type {
  CapabilityKind,
  CapabilityStatusChangedPayload,
  EventTopic,
  IndexCacheStatus,
  PartialResult,
  PartialResultKind,
} from '@/types/models';

export const CAPABILITY_MATRIX_QUERY_KEY = ['capability', 'matrix'] as const;

export const PROJECTION_EVENT_TOPICS = [
  'case-opened',
  'case-closed',
  'job-completed',
  'job-failed',
  'job-cancelled',
  'data-source-imported',
  'artifact-added',
  'timeline-updated',
  'search-index-progress',
  'capability-status-changed',
] as const satisfies readonly EventTopic[];

export const timelineQueryKeys = {
  root: ['timeline'] as const,
  eventsRoot: ['timeline', 'events'] as const,
  events: (request?: { offset?: number; limit?: number; timeStart?: string; timeEnd?: string; eventType?: string }) =>
    [
      'timeline',
      'events',
      request?.offset ?? 0,
      request?.limit ?? 100,
      request?.timeStart ?? null,
      request?.timeEnd ?? null,
      request?.eventType ?? null,
    ] as const,
  facets: (request?: { timeStart?: string; timeEnd?: string; eventType?: string; bucketCount?: number }) =>
    [
      'timeline',
      'facets',
      request?.timeStart ?? null,
      request?.timeEnd ?? null,
      request?.eventType ?? null,
      request?.bucketCount ?? 60,
    ] as const,
};

export const projectionQueryKeys = {
  case: ['case'] as const,
  caseCurrent: ['case', 'current'] as const,
  caseMetrics: ['case', 'metrics'] as const,
  caseDataSources: ['case', 'data-sources'] as const,
  caseRecentObjects: ['case', 'recent-objects'] as const,
  caseRecentCases: ['case', 'recent-cases'] as const,
  files: ['files'] as const,
  timeline: timelineQueryKeys.root,
  artifacts: ['artifacts'] as const,
  search: ['search'] as const,
  reports: ['reports'] as const,
  analysis: ['analysis'] as const,
  jobWarnings: ['jobs', 'warnings'] as const,
  jobTrace: ['jobs', 'trace'] as const,
  graph: ['graph'] as const,
  capabilities: CAPABILITY_MATRIX_QUERY_KEY,
  linuxEvidenceSets: ['linux-evidence-sets'] as const,
  linuxEvidenceSetSummary: ['linux-evidence-set-summary'] as const,
  linuxEvidenceSetEvents: ['linux-evidence-set-events'] as const,
  emulationPreflight: ['emulation', 'preflight'] as const,
} as const;

type ProjectionKey = keyof typeof projectionQueryKeys;
type QueryInvalidator = Pick<QueryClient, 'cancelQueries' | 'invalidateQueries'>;

const importProjectionKeys: ProjectionKey[] = [
  'caseDataSources',
  'caseMetrics',
  'caseRecentObjects',
  'files',
  'timeline',
  'artifacts',
  'search',
  'analysis',
  'graph',
  'capabilities',
  'linuxEvidenceSets',
  'linuxEvidenceSetSummary',
  'linuxEvidenceSetEvents',
];

const postJobProjectionKeys: ProjectionKey[] = [...importProjectionKeys, 'jobWarnings', 'jobTrace'];

const partialResultProjectionKeys: Record<PartialResultKind, ProjectionKey[]> = {
  fileTree: ['files', 'caseDataSources'],
  fileRows: ['files', 'caseDataSources'],
  partition: ['files', 'caseDataSources'],
  timelineEvents: ['timeline'],
  timelineBuckets: ['timeline'],
  artifactFamily: ['artifacts', 'timeline'],
  searchIndex: ['search'],
  evidenceHash: ['caseMetrics', 'caseDataSources', 'reports'],
};

function invalidateProjectionKeys(queryClient: QueryInvalidator, keys: ProjectionKey[]) {
  const uniqueKeys = Array.from(new Set(keys));
  uniqueKeys.forEach((key) => {
    queryClient.invalidateQueries({ queryKey: projectionQueryKeys[key] });
  });
}

/**
 * High-frequency backend events (artifact-added, partial results, cache
 * status) can fire dozens of times per second during import/extraction.
 * Invalidating whole query families per event turns into an IPC/refetch
 * storm, so those paths are coalesced: keys accumulate and flush once per
 * interval with the union of all pending families.
 */
const EVENT_INVALIDATION_COALESCE_MS = 300;

let scheduledKeys = new Set<ProjectionKey>();
let scheduledClient: QueryInvalidator | undefined;
let scheduledFlush: ReturnType<typeof setTimeout> | undefined;

function flushScheduledInvalidations() {
  if (scheduledFlush !== undefined) {
    clearTimeout(scheduledFlush);
  }
  scheduledFlush = undefined;
  const client = scheduledClient;
  const keys = Array.from(scheduledKeys);
  scheduledKeys = new Set();
  scheduledClient = undefined;
  if (client) {
    invalidateProjectionKeys(client, keys);
  }
}

function scheduleProjectionInvalidation(queryClient: QueryInvalidator, keys: ProjectionKey[]) {
  keys.forEach((key) => scheduledKeys.add(key));
  scheduledClient = queryClient;
  scheduledFlush ??= setTimeout(flushScheduledInvalidations, EVENT_INVALIDATION_COALESCE_MS);
}

export function invalidateImportProjectionQueries(queryClient: QueryInvalidator) {
  invalidateProjectionKeys(queryClient, importProjectionKeys);
}

export function invalidatePostJobProjectionQueries(queryClient: QueryInvalidator) {
  invalidateProjectionKeys(queryClient, postJobProjectionKeys);
}

export function invalidatePartialResultQueries(
  queryClient: QueryInvalidator,
  result: Pick<PartialResult, 'kind' | 'freshness'>,
) {
  if (result.freshness === 'deferred') {
    return;
  }

  scheduleProjectionInvalidation(queryClient, partialResultProjectionKeys[result.kind]);
}

export function invalidateCacheStatusQueries(
  queryClient: QueryInvalidator,
  status: Pick<IndexCacheStatus, 'cacheKey' | 'state'>,
) {
  if (status.state === 'deferred' || status.state === 'pending') {
    return;
  }

  if (status.cacheKey.toLowerCase().includes('timeline')) {
    scheduleProjectionInvalidation(queryClient, ['timeline']);
  } else if (status.cacheKey.toLowerCase().includes('search')) {
    scheduleProjectionInvalidation(queryClient, ['search']);
  }
}

const capabilityProjectionKeys: Record<CapabilityKind, ProjectionKey[]> = {
  catalog: ['files', 'caseDataSources', 'caseMetrics', 'emulationPreflight'],
  search: ['search'],
  timeline: ['timeline', 'graph', 'analysis', 'caseMetrics', 'caseRecentObjects'],
  graph: ['graph'],
  platformFacts: ['analysis', 'caseDataSources', 'caseMetrics'],
  artifacts: ['artifacts', 'analysis', 'timeline', 'graph', 'caseMetrics', 'caseRecentObjects'],
};

function invalidateCapabilityPublication(queryClient: QueryInvalidator, payload: CapabilityStatusChangedPayload) {
  const { caseId, dataSourceId, capability, state } = payload;
  if (!caseId) return;
  void queryClient.invalidateQueries({ queryKey: [...CAPABILITY_MATRIX_QUERY_KEY, caseId] });
  if (!capability || !state || ['pending', 'queued', 'running'].includes(state)) return;
  const roots = new Set<string>(capabilityProjectionKeys[capability].map((key) => projectionQueryKeys[key][0]));
  const filters: QueryFilters = {
    predicate: ({ queryKey }) => {
      const family = queryKey[0];
      if (typeof family !== 'string' || !roots.has(family)) return false;
      const caseIndex = ['artifacts', 'timeline', 'search'].includes(family) ? 1 : 2;
      if (queryKey[caseIndex] !== caseId) return false;
      return family !== 'analysis' || !dataSourceId || typeof queryKey[3] !== 'string' || queryKey[3] === dataSourceId;
    },
  };
  void queryClient.cancelQueries(filters);
  void queryClient.invalidateQueries(filters);
}

export function invalidateEventProjectionQueries(
  queryClient: QueryInvalidator,
  topic: EventTopic,
  payload: CapabilityStatusChangedPayload = {},
) {
  switch (topic) {
    case 'data-source-imported':
      invalidateImportProjectionQueries(queryClient);
      void queryClient.invalidateQueries({ queryKey: projectionQueryKeys.emulationPreflight });
      return;
    case 'timeline-updated':
      if (payload.caseId) {
        invalidateCapabilityPublication(queryClient, { ...payload, capability: 'timeline', state: 'ready' });
      } else {
        scheduleProjectionInvalidation(queryClient, ['timeline', 'capabilities']);
      }
      return;
    case 'capability-status-changed':
      invalidateCapabilityPublication(queryClient, payload);
      return;
    case 'artifact-added':
      scheduleProjectionInvalidation(queryClient, ['artifacts', 'timeline']);
      return;
    case 'search-index-progress':
      scheduleProjectionInvalidation(queryClient, ['search']);
      return;
    case 'job-completed':
    case 'job-failed':
    case 'job-cancelled':
      // Terminal events are one-shot: flush any coalesced work first so the
      // final state is invalidated after all in-flight progress updates.
      flushScheduledInvalidations();
      invalidatePostJobProjectionQueries(queryClient);
      return;
    case 'case-opened':
    case 'case-closed':
      flushScheduledInvalidations();
      queryClient.invalidateQueries();
      return;
    default:
      return;
  }
}
