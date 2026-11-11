import { apiClient } from './client';
import { COMMANDS } from './commands';
import type { ContentIndexCoverage, EvidenceTask, EvidenceTaskRequest } from '@/types/evidenceTask';
export const startEvidenceTask = (request: EvidenceTaskRequest): Promise<EvidenceTask> =>
  apiClient.request(COMMANDS.evidenceTasks.START, { request });
export const getEvidenceTask = (taskId: string): Promise<EvidenceTask> =>
  apiClient.request(COMMANDS.evidenceTasks.GET, { taskId });
export const cancelEvidenceTask = (taskId: string): Promise<EvidenceTask> =>
  apiClient.request(COMMANDS.evidenceTasks.CANCEL, { taskId });
export function getContentIndexCoverage(dataSourceId: string): Promise<ContentIndexCoverage> {
  return apiClient.request(COMMANDS.evidenceTasks.COVERAGE, { dataSourceId });
}
