import type { ApplicationLogLevel, ApplicationLogRecord, ApplicationLogSnapshot } from '@/types/applicationLog';

export const LOG_LEVELS: ApplicationLogLevel[] = ['info', 'warning', 'error'];
const MAX_RECORDS = { info: 1000, warning: 500, error: 500 };
const MAX_BYTES = { info: 2 * 1024 * 1024, warning: 1024 * 1024, error: 1024 * 1024 };

export interface LogBuffer {
  sessionId: string;
  rows: Record<ApplicationLogLevel, ApplicationLogRecord[]>;
  latest?: ApplicationLogSnapshot;
  forward: string[];
  backward: string[];
}

export function emptyLogBuffer(): LogBuffer {
  return { sessionId: '', rows: { info: [], warning: [], error: [] }, forward: [], backward: [] };
}

export function mergeLogSnapshot(current: LogBuffer, snapshot: ApplicationLogSnapshot, history = false): LogBuffer {
  const base = current.sessionId === snapshot.sessionId ? current : emptyLogBuffer();
  const cursors = {
    forward: history && base.forward.length ? base.forward : snapshot.pages.map((page) => page.forwardCursor),
    backward: history || !base.backward.length ? snapshot.pages.map((page) => page.backwardCursor) : base.backward,
  };
  if (snapshot.pages.every((page) => page.records.length === 0)) {
    return { ...base, ...cursors, sessionId: snapshot.sessionId, latest: snapshot };
  }
  const rows = { ...base.rows };
  for (const page of snapshot.pages) {
    const unique = new Map<number, ApplicationLogRecord>();
    for (const record of [...rows[page.level], ...page.records]) unique.set(record.sequence, record);
    const ordered = [...unique.values()].sort((left, right) => left.sequence - right.sequence);
    rows[page.level] = history ? ordered.slice(0, MAX_RECORDS[page.level]) : ordered.slice(-MAX_RECORDS[page.level]);
  }
  for (const level of LOG_LEVELS) {
    let bytes = 0;
    const retained: ApplicationLogRecord[] = [];
    const candidates = history ? rows[level] : [...rows[level]].reverse();
    for (const record of candidates) {
      const size = new TextEncoder().encode(JSON.stringify(record)).length;
      if (bytes + size > MAX_BYTES[level]) break;
      bytes += size;
      retained.push(record);
    }
    rows[level] = retained.sort((left, right) => left.sequence - right.sequence);
  }
  return {
    sessionId: snapshot.sessionId,
    rows,
    latest: snapshot,
    ...cursors,
  };
}
