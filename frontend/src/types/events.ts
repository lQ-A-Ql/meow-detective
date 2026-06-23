export type EventTopic =
  | 'case-opened'
  | 'case-closed'
  | 'job-created'
  | 'job-started'
  | 'job-progress'
  | 'job-completed'
  | 'job-failed'
  | 'job-cancelled'
  | 'data-source-imported'
  | 'artifact-added'
  | 'timeline-updated'
  | 'search-index_progress'
  | 'partition-progress'
  | 'import-phase-progress'
  | 'import-partial-result'
  | 'job-cancellation'
  | 'cache-index-status'
  | 'performance-report-ready';

export interface EventEnvelope<T = unknown> {
  eventId: string;
  topic: EventTopic;
  ts: string;
  payload: T;
}
