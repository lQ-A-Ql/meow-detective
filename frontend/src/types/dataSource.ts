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
  sourceHash?: string;
  hashStatus?: string;
  canonicalPath?: string;
  evidenceSize?: number;
  readerKind?: string;
  provenanceStatus?: string;
  warnings?: string[];
  partitions?: DataSourcePartition[];
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
