import type { CalculateEvidenceDigestRequest, EvidenceDigest } from '@/types/digest';
import { COMMANDS } from './commands';
import { apiClient } from './client';
import { getEvidenceTask } from './evidence-tasks';
import type { EvidenceTask } from '@/types/evidenceTask';

export async function calculateEvidenceDigest(
  request: CalculateEvidenceDigestRequest,
): Promise<EvidenceDigest> {
  let task = await apiClient.request<EvidenceTask>(COMMANDS.files.CALCULATE_EVIDENCE_DIGEST, { request });
  while (task.state === 'queued' || task.state === 'running' || task.state === 'cancelling') {
    await new Promise<void>((resolve) => {
      const poll = window.setInterval(() => { window.clearInterval(poll); resolve(); }, 250);
    });
    task = await getEvidenceTask(task.taskId);
  }
  if (task.result?.kind === 'digest') return task.result.result;
  throw { code: task.errorCode ?? 'digest_failed', category: task.state === 'cancelled' ? 'cancelled' : 'internal', message: 'Evidence digest task did not complete' };
}

export interface DigestResult { digest: EvidenceDigest; sourceRevision: string; stale: boolean }
export function listEvidenceDigests(request: { dataSourceId: string; fileId?: string; partitionIndex?: number }): Promise<DigestResult[]> {
  return apiClient.request(COMMANDS.files.LIST_EVIDENCE_DIGESTS, { request });
}
