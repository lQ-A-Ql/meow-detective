export type ApplicationLogLevel = 'info' | 'warning' | 'error';
export type ApplicationLogDirection = 'latest' | 'forward' | 'backward';

export interface ApplicationLogFilter {
  text: string;
  target: string;
  caseId: string;
  dataSourceId: string;
  jobId: string;
}

export interface ApplicationLogRequest {
  direction: ApplicationLogDirection;
  cursors: string[];
  filter: ApplicationLogFilter;
  limitPerLevel: number;
}

export interface ApplicationLogRecord {
  sessionId: string;
  sequence: number;
  timestamp: string;
  level: ApplicationLogLevel;
  target: string;
  message: string;
  fields: Record<string, string>;
  threadId: string;
  threadName?: string;
  truncated: boolean;
}

export interface ApplicationLogPage {
  level: ApplicationLogLevel;
  records: ApplicationLogRecord[];
  forwardCursor: string;
  backwardCursor: string;
  hasMore: boolean;
  gap: boolean;
  emitted: number;
  evicted: number;
  truncated: number;
  fileDropped: number;
}

export interface ApplicationLogWriter {
  failure?: string;
  failedRecords: number;
  queuedRecords: number;
  queuedBytes: number;
  writtenSequence: number;
}

export interface ApplicationLogSnapshot {
  sessionId: string;
  upperSequence: number;
  pages: ApplicationLogPage[];
  writer: ApplicationLogWriter;
}
