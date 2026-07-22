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
  | 'search-index-progress'
  | 'partition-progress'
  | 'import-phase-progress'
  | 'import-partial-result'
  | 'job-cancellation'
  | 'cache-index-status'
  | 'performance-report-ready'
  | 'analysis-extraction-progress';

export interface EventEnvelope<T = unknown> {
  eventId: string;
  topic: EventTopic;
  ts: string;
  payload: T;
}
