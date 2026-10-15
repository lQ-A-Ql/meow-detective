import { apiClient } from './client';
import { COMMANDS } from './commands';
import type { EvidenceTask, EvidenceTaskRequest } from '@/types/evidenceTask';
export const startEvidenceTask = (request: EvidenceTaskRequest): Promise<EvidenceTask> =>
  apiClient.request(COMMANDS.evidenceTasks.START, { request });
export const getEvidenceTask = (taskId: string): Promise<EvidenceTask> =>
  apiClient.request(COMMANDS.evidenceTasks.GET, { taskId });
export const cancelEvidenceTask = (taskId: string): Promise<EvidenceTask> =>
  apiClient.request(COMMANDS.evidenceTasks.CANCEL, { taskId });
import type { ContentIndexSummary } from '@/types/evidenceTask';
export interface ContentIndexCoverage { summary: ContentIndexSummary; stale: boolean; sourceRevision?: string }
export function getContentIndexCoverage(dataSourceId: string): Promise<ContentIndexCoverage> { return apiClient.request(COMMANDS.evidenceTasks.COVERAGE, { dataSourceId }); }
