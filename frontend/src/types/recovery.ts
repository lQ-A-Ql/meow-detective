export type RecoveryScanState = 'complete' | 'partial' | 'failed';
export type RecoveryCompleteness = 'metadata_only' | 'partial' | 'complete';
export type RecoveryAllocationState =
  | 'unverified'
  | 'free'
  | 'allocated'
  | 'partially_overwritten';
export type RecoveryIssueSeverity = 'info' | 'warning' | 'error';
export type RecoveryHashAlgorithm = 'md5' | 'sha1' | 'sha256';

export interface RecoveryProvenanceRange {
  ordinal: number;
  rangeRole: 'metadata' | 'content';
  sourceKind: 'filesystem' | 'journal' | 'log';
  logicalOffset: number;
  sourceOffset: number;
  physicalOffset?: number;
  length: number;
  allocationState: RecoveryAllocationState;
  sha256?: string;
}

export interface RecoveryIssue {
  ordinal: number;
  severity: RecoveryIssueSeverity;
  code: string;
  message: string;
  logOffset?: number;
  sequence?: number;
}

export interface DeletedFileRecovery {
  id: string;
  dataSourceId: string;
  partitionIndex: number;
  filesystemType: 'ext4' | 'xfs' | 'ntfs';
  filesystemUuid?: string;
  inode: string;
  originalPath?: string;
  entryType?: 'file' | 'directory' | 'symlink';
  mode?: number;
  mftSequence?: number;
  deletedAtUnix?: number;
  declaredSize: number;
  recoverableBytes: number;
  completeness: RecoveryCompleteness;
  allocationState: RecoveryAllocationState;
  recoveryMethod: string;
  confidence: number;
  transactionId?: string;
  logSequence?: number;
  logCycle?: number;
  contentMd5?: string;
  contentSha1?: string;
  contentSha256?: string;
  provenanceRanges: RecoveryProvenanceRange[];
  warnings: string[];
}

export interface DeletedRecoveryScan {
  id: string;
  dataSourceId: string;
  partitionIndex: number;
  filesystemType: 'ext4' | 'xfs' | 'ntfs';
  filesystemUuid?: string;
  parserVersion: string;
  logKind: 'internal_journal' | 'internal_log';
  snapshotIdentitySha256: string;
  state: RecoveryScanState;
  transactionCount: number;
  candidateCount: number;
  warnings: string[];
  startedAt: string;
  completedAt: string;
  issues: RecoveryIssue[];
}

export interface DeletedRecoveryPage {
  scan: DeletedRecoveryScan;
  recoveries: DeletedFileRecovery[];
  offset: number;
  limit: number;
  total: number;
}

export interface DeletedRecoveryHashSearch {
  algorithm: RecoveryHashAlgorithm;
  normalizedHash: string;
  matches: DeletedFileRecovery[];
}

export interface DeletedRecoveryFailure {
  partitionIndex: number;
  filesystemType: string;
  code: string;
  message: string;
}

export interface DeletedRecoveryRun {
  dataSourceId: string;
  scans: DeletedRecoveryScan[];
  failures: DeletedRecoveryFailure[];
}

export interface DeletedRecoveryContentRange {
  recoveryId: string;
  offset: number;
  bytesBase64: string;
  bytesRead: number;
  declaredSize: number;
  eof: boolean;
  verifiedRangeOrdinals: number[];
}

export interface DeletedRecoveryExport {
  recoveryId: string;
  bytesWritten: number;
  sha256: string;
}
