export interface LedgerEntry {
  id: string;
  scopeKey: string;
  caseId?: string;
  auditId: string;
  sequence: number;
  actorId: string;
  action: string;
  resourceType: string;
  resourceId?: string;
  details: string;
  previousHash: string;
  entryHash: string;
  createdAt: string;
}

export interface LedgerBatch {
  id: string;
  scopeKey: string;
  caseId?: string;
  startSequence: number;
  endSequence: number;
  entryCount: number;
  merkleRoot: string;
  headHash: string;
  createdAt: string;
}

export interface LedgerBatchVerification {
  valid: boolean;
  batchCount: number;
  firstError?: string;
}

export interface LedgerVerification {
  valid: boolean;
  entryCount: number;
  headHash?: string;
  firstError?: string;
  batches: LedgerBatchVerification;
}

export interface LedgerSnapshot {
  entries: LedgerEntry[];
  batches: LedgerBatch[];
  verification: LedgerVerification;
}

export interface LedgerProofStep {
  siblingHash: string;
  siblingOnLeft: boolean;
}

export interface LedgerProof {
  batch: LedgerBatch;
  sequence: number;
  entryHash: string;
  leafIndex: number;
  leafCount: number;
  steps: LedgerProofStep[];
  merkleRoot: string;
}
