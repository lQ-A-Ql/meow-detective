export interface DataSourceSummary {
  id: string;
  name: string;
  kind: 'e01' | 'raw' | 'logical_directory' | string;
  sourcePath: string;
  importedAt: string;
  fileCount?: number;
  storageModel?: string;
  sourceDbRelPath?: string;
  indexRelPath?: string;
  stagingRelPath?: string;
  platform: 'windows' | 'linux';
  profile?: string;
  importState?: 'pending' | 'importing' | 'ready' | 'ready_metadata' | 'failed' | string;
  schemaVersion?: string;
  lastError?: string;
  processing?: DataSourceProcessingSummary;
  sourceHash?: string;
  hashStatus?: string;
  canonicalPath?: string;
  evidenceSize?: number;
  readerKind?: string;
  provenanceStatus?: string;
  warnings?: string[];
  partitions?: DataSourcePartition[];
}

export interface DataSourceProcessingSummary {
  state: 'pending' | 'running' | 'ready' | 'failed' | 'deferred';
  totalCount: number;
  readyCount: number;
  pendingCount: number;
  runningCount: number;
  failedCount: number;
  deferredCount: number;
  lastError?: string;
  phases: DataSourceProcessingPhase[];
}

export interface DataSourceProcessingPhase {
  phase: 'catalog' | 'graph' | 'platform' | 'artifacts' | 'timeline' | 'search';
  state: 'pending' | 'running' | 'ready' | 'failed' | 'deferred';
  version: number;
  stats: Record<string, unknown>;
  lastError?: string;
  startedAt?: string;
  completedAt?: string;
  heartbeatAt?: string;
  leaseExpiresAt?: string;
  updatedAt: string;
}

export interface DataSourcePartition {
  index: number;
  name: string;
  kindLabel: string;
  status: string;
  offset: number;
  length: number;
  typeGuid?: string;
  filesystem?: string;
  unlockHint?: string;
}

export interface BitLockerProtector {
  code: number;
  kind: 'clearKey' | 'recoveryPassword' | 'password' | 'tpm' | 'startupKey' | 'unknown';
  label: string;
  unlockable: boolean;
}

export interface BitLockerVolumeStatus {
  dataSourceId: string;
  partitionIndex: number;
  unlocked: boolean;
  encryptionMethod: string;
  encryptionMethodCode: number;
  decryptable: boolean;
  bytesPerSector: number;
  metadataFingerprint: string;
  metadataCopyCount: number;
  protectors: BitLockerProtector[];
  supportsPassword: boolean;
  supportsRecoveryPassword: boolean;
  storedKeyAvailable: boolean;
  plaintextFilesystem?: string;
}

export interface BitLockerCatalogImport {
  volume: BitLockerVolumeStatus;
  imported: boolean;
  fileCount?: number;
  directoryCount?: number;
  totalSize?: number;
  warnings: string[];
}
