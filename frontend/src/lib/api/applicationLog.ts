import { apiClient } from './client';
import type {
  ApplicationLogRecord,
  ApplicationLogRequest,
  ApplicationLogSnapshot,
  ApplicationLogWriter,
} from '@/types/applicationLog';

export function readApplicationLogs(request: ApplicationLogRequest) {
  return apiClient.request<ApplicationLogSnapshot>('read_application_logs', { request });
}

export function getApplicationLog(sessionId: string, sequence: number) {
  return apiClient.request<ApplicationLogRecord>('get_application_log', { sessionId, sequence });
}

export function reopenApplicationLogWriter() {
  return apiClient.request<ApplicationLogWriter>('reopen_application_log_writer');
}
