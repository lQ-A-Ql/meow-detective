import type { QueryClient } from '@tanstack/react-query';
import type { EventTopic, IndexCacheStatus, PartialResult, PartialResultKind } from '@/types/models';

export const timelineQueryKeys = {
  root: ['timeline'] as const,
  eventsRoot: ['timeline', 'events'] as const,
  events: (request?: {
    offset?: number;
    limit?: number;
    timeStart?: string;
    timeEnd?: string;
    eventType?: string;
  }) => [
    'timeline',
    'events',
    request?.offset ?? 0,
    request?.limit ?? 100,
    request?.timeStart ?? null,
    request?.timeEnd ?? null,
    request?.eventType ?? null,
  ] as const,
};

export const projectionQueryKeys = {
  case: ['case'] as const,
  files: ['files'] as const,
  timeline: timelineQueryKeys.root,
  artifacts: ['artifacts'] as const,
  search: ['search'] as const,
  reports: ['reports'] as const,
  jobWarnings: ['jobs', 'warnings'] as const,
  jobTrace: ['jobs', 'trace'] as const,
} as const;

type ProjectionKey = keyof typeof projectionQueryKeys;
type QueryInvalidator = Pick<QueryClient, 'invalidateQueries'>;

const importProjectionKeys: ProjectionKey[] = ['case', 'files', 'timeline', 'artifacts', 'search'];
const postJobProjectionKeys: ProjectionKey[] = [...importProjectionKeys, 'jobWarnings', 'jobTrace'];

const partialResultProjectionKeys: Record<PartialResultKind, ProjectionKey[]> = {
  fileTree: ['files', 'case'],
  fileRows: ['files', 'case'],
  partition: ['files', 'case'],
  timelineEvents: ['timeline'],
  timelineBuckets: ['timeline'],
  artifactFamily: ['artifacts', 'timeline'],
  searchIndex: ['search'],
  evidenceHash: ['case', 'reports'],
};

function invalidateProjectionKeys(queryClient: QueryInvalidator, keys: ProjectionKey[]) {
  const uniqueKeys = Array.from(new Set(keys));
  uniqueKeys.forEach((key) => {
    queryClient.invalidateQueries({ queryKey: projectionQueryKeys[key] });
  });
}

export function invalidateImportProjectionQueries(queryClient: QueryInvalidator) {
  invalidateProjectionKeys(queryClient, importProjectionKeys);
}

export function invalidatePostJobProjectionQueries(queryClient: QueryInvalidator) {
  invalidateProjectionKeys(queryClient, postJobProjectionKeys);
}

export function invalidatePartialResultQueries(queryClient: QueryInvalidator, result: Pick<PartialResult, 'kind' | 'freshness'>) {
  if (result.freshness === 'deferred') {
    return;
  }

  invalidateProjectionKeys(queryClient, partialResultProjectionKeys[result.kind]);
}

export function invalidateCacheStatusQueries(queryClient: QueryInvalidator, status: Pick<IndexCacheStatus, 'cacheKey' | 'state'>) {
  if (status.state === 'deferred' || status.state === 'pending') {
    return;
  }

  if (status.cacheKey.toLowerCase().includes('timeline')) {
    invalidateProjectionKeys(queryClient, ['timeline']);
  } else if (status.cacheKey.toLowerCase().includes('search')) {
    invalidateProjectionKeys(queryClient, ['search']);
  }
}

export function invalidateEventProjectionQueries(queryClient: QueryInvalidator, topic: EventTopic) {
  switch (topic) {
    case 'data-source-imported':
      invalidateImportProjectionQueries(queryClient);
      return;
    case 'timeline-updated':
      invalidateProjectionKeys(queryClient, ['timeline']);
      return;
    case 'artifact-added':
      invalidateProjectionKeys(queryClient, ['artifacts', 'timeline']);
      return;
    case 'search-index_progress':
      invalidateProjectionKeys(queryClient, ['search']);
      return;
    case 'case-opened':
    case 'case-closed':
      queryClient.invalidateQueries();
      return;
    default:
      return;
  }
}
