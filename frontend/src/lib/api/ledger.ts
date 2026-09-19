import type { LedgerBatch, LedgerProof, LedgerSnapshot } from '@/types/models';
import { COMMANDS } from './commands';
import { apiClient } from './client';

export function getLedgerSnapshot(params?: {
  limit?: number;
  offset?: number;
}): Promise<LedgerSnapshot> {
  return apiClient.request(COMMANDS.ledger.GET_LEDGER_SNAPSHOT, {
    limit: params?.limit ?? null,
    offset: params?.offset ?? null,
  });
}

export function sealLedgerBatch(): Promise<LedgerBatch | null> {
  return apiClient.request(COMMANDS.ledger.SEAL_LEDGER_BATCH);
}

export function getLedgerProof(batchId: string, sequence: number): Promise<LedgerProof> {
  return apiClient.request(COMMANDS.ledger.GET_LEDGER_PROOF, { batchId, sequence });
}
