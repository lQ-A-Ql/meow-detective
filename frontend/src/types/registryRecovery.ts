export type RegistryRecoveryStatus = 'notNeeded' | 'recovered' | 'partial' | 'failed';
export type RegistryLogSlot = 'log1' | 'log2';
export type RegistryLogFormat = 'hvle' | 'dirt';
export type RegistryLogStatus = 'applied' | 'notNeeded' | 'stale' | 'invalid' | 'unavailable' | 'duplicate' | 'notSelected' | 'missing' | 'ambiguous' | 'locked' | 'tooLarge';
export interface RegistryLogProvenance {
  slot: RegistryLogSlot;
  status: RegistryLogStatus;
  appliedEntries: number;
  appliedPages: number;
  fileId?: string;
  evidencePath?: string;
  byteLength?: number;
  sha256?: string;
  format?: RegistryLogFormat;
  firstSequence?: number;
  lastSequence?: number;
  diagnosticCode?: string;
}
export interface RegistryRecovery {
  status: RegistryRecoveryStatus;
  basePrimarySequence: number;
  baseSecondarySequence: number;
  baseChecksumValid: boolean;
  appliedEntries: number;
  appliedPages: number;
  logs: RegistryLogProvenance[];
  diagnosticCodes: string[];
  baseSha256?: string;
  resultSha256?: string;
  resultSequence?: number;
}
