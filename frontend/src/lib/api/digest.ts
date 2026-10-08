import type { CalculateEvidenceDigestRequest, EvidenceDigest } from '@/types/digest';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export function calculateEvidenceDigest(
  request: CalculateEvidenceDigestRequest,
): Promise<EvidenceDigest> {
  return apiClient.request<EvidenceDigest>(COMMANDS.files.CALCULATE_EVIDENCE_DIGEST, { request });
}
