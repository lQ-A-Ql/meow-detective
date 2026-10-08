export type DigestScope =
  | 'containerFile'
  | 'containerSet'
  | 'logicalDisk'
  | 'partition'
  | 'file'
  | 'derivedEvidence';

export type DigestAlgorithm = 'md5' | 'sha1' | 'sha256' | 'sm3';

export interface CalculateEvidenceDigestRequest {
  scope: DigestScope;
  algorithm: DigestAlgorithm;
  dataSourceId?: string;
  fileId?: string;
  partitionIndex?: number;
}

export interface EvidenceDigest {
  scope: DigestScope;
  algorithm: DigestAlgorithm;
  value: string;
  byteLength: number;
  status: 'running' | 'completed' | 'cancelled' | 'failed';
}
